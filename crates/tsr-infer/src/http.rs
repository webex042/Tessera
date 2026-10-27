//! Minimal HTTP/1.1 client over a plain TCP socket, for the loopback sidecar only (plan §3, §7.3).
//!
//! Deliberately small: one request per connection (`Connection: close`, so a response can never be
//! matched to the wrong request), `Content-Length` and chunked bodies, a wall-clock deadline for the
//! whole exchange, a silence timeout between reads, size limits in both directions, and a
//! cancellation check every 50 ms. No TLS, no proxies, no redirects, no DNS: callers pass a socket
//! address, and [`crate::client::LoopbackClient`] only accepts loopback ones.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use crate::cancel::{CancelReason, CancelToken};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub connect_timeout: Duration,
    /// Longest silence between two reads (a hung server).
    pub idle_timeout: Duration,
    /// The whole exchange, connect to last byte.
    pub total_timeout: Duration,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            connect_timeout: Duration::from_secs(2),
            idle_timeout: Duration::from_secs(60),
            total_timeout: Duration::from_secs(180),
            max_request_bytes: 8 << 20,
            max_response_bytes: 4 << 20,
        }
    }
}

/// Content-free transport errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HttpError {
    #[error("connect failed: {0:?}")]
    Connect(io::ErrorKind),
    #[error("timed out")]
    Timeout,
    #[error("cancelled: {0:?}")]
    Cancelled(CancelReason),
    #[error("request larger than the limit")]
    RequestTooLarge,
    #[error("response larger than the limit")]
    ResponseTooLarge,
    #[error("malformed response: {0}")]
    Malformed(&'static str),
    #[error("i/o error: {0:?}")]
    Io(io::ErrorKind),
    #[error("invalid header value")]
    BadHeader,
}

pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub headers: &'a [(&'a str, &'a str)],
    pub body: Option<&'a [u8]>,
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        header(&self.headers, name)
    }
}

pub fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

const POLL: Duration = Duration::from_millis(50);
const MAX_LINE: usize = 8 << 10;
const MAX_HEADERS: usize = 64;

struct Reader<'a> {
    stream: TcpStream,
    deadline: Instant,
    idle: Duration,
    last_progress: Instant,
    cancel: Option<&'a CancelToken>,
    buf: Box<[u8]>,
    pos: usize,
    len: usize,
}

impl Reader<'_> {
    fn check(&self) -> Result<(), HttpError> {
        if let Some(r) = self.cancel.and_then(|c| c.reason()) {
            return Err(HttpError::Cancelled(r));
        }
        let now = Instant::now();
        if now >= self.deadline || now.duration_since(self.last_progress) > self.idle {
            return Err(HttpError::Timeout);
        }
        Ok(())
    }

    /// Refill the buffer; `Ok(false)` on end of stream.
    fn fill(&mut self) -> Result<bool, HttpError> {
        loop {
            self.check()?;
            let wait = POLL.min(self.deadline.saturating_duration_since(Instant::now()));
            self.stream
                .set_read_timeout(Some(wait.max(Duration::from_millis(1))))
                .map_err(|e| HttpError::Io(e.kind()))?;
            match self.stream.read(&mut self.buf) {
                Ok(0) => return Ok(false),
                Ok(n) => {
                    self.pos = 0;
                    self.len = n;
                    self.last_progress = Instant::now();
                    return Ok(true);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(HttpError::Io(e.kind())),
            }
        }
    }

    fn byte(&mut self) -> Result<Option<u8>, HttpError> {
        if self.pos == self.len && !self.fill()? {
            return Ok(None);
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(Some(b))
    }

    /// One line without the trailing CRLF.
    fn line(&mut self) -> Result<Vec<u8>, HttpError> {
        let mut out = Vec::new();
        loop {
            match self.byte()? {
                None => return Err(HttpError::Malformed("eof in line")),
                Some(b'\n') => {
                    if out.last() == Some(&b'\r') {
                        out.pop();
                    }
                    return Ok(out);
                }
                Some(b) => {
                    if out.len() >= MAX_LINE {
                        return Err(HttpError::Malformed("line too long"));
                    }
                    out.push(b);
                }
            }
        }
    }

    /// Up to `max` bytes of whatever is available (at least one unless at end of stream).
    fn chunk(&mut self, max: usize) -> Result<&[u8], HttpError> {
        if self.pos == self.len && !self.fill()? {
            return Ok(&[]);
        }
        let n = (self.len - self.pos).min(max);
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
}

fn valid_header_value(v: &str) -> bool {
    !v.bytes().any(|b| b == b'\r' || b == b'\n' || b == 0)
}

/// Send a request and stream the response body to `on_body` as it arrives (chunked encoding is
/// decoded). Returns the status and headers. `on_body` may return an error to stop early, which
/// closes the connection (llama-server stops generating when the client disconnects).
pub fn send_stream(
    addr: SocketAddr,
    req: &Request<'_>,
    limits: &Limits,
    cancel: Option<&CancelToken>,
    on_body: &mut dyn FnMut(&[u8]) -> Result<(), HttpError>,
) -> Result<(u16, Vec<(String, String)>), HttpError> {
    let body = req.body.unwrap_or(&[]);
    if body.len() > limits.max_request_bytes {
        return Err(HttpError::RequestTooLarge);
    }
    for (k, v) in req.headers {
        if !valid_header_value(k) || !valid_header_value(v) || k.contains(':') {
            return Err(HttpError::BadHeader);
        }
    }
    if !valid_header_value(req.path) || !req.path.starts_with('/') || req.path.contains(' ') {
        return Err(HttpError::BadHeader);
    }
    if let Some(r) = cancel.and_then(|c| c.reason()) {
        return Err(HttpError::Cancelled(r));
    }
    let start = Instant::now();
    let stream = TcpStream::connect_timeout(&addr, limits.connect_timeout).map_err(|e| HttpError::Connect(e.kind()))?;
    let _ = stream.set_nodelay(true);

    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nAccept-Encoding: identity\r\n",
        req.method, req.path
    );
    for (k, v) in req.headers {
        head.push_str(k);
        head.push_str(": ");
        head.push_str(v);
        head.push_str("\r\n");
    }
    if req.body.is_some() {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    {
        let mut w = &stream;
        w.set_write_timeout(Some(limits.idle_timeout))
            .map_err(|e| HttpError::Io(e.kind()))?;
        w.write_all(head.as_bytes()).map_err(|e| HttpError::Io(e.kind()))?;
        // Written in slices so cancellation and the deadline are honoured for big image payloads.
        for part in body.chunks(256 << 10) {
            if let Some(r) = cancel.and_then(|c| c.reason()) {
                return Err(HttpError::Cancelled(r));
            }
            if start.elapsed() > limits.total_timeout {
                return Err(HttpError::Timeout);
            }
            w.write_all(part).map_err(|e| HttpError::Io(e.kind()))?;
        }
        w.flush().map_err(|e| HttpError::Io(e.kind()))?;
    }

    let mut r = Reader {
        stream,
        deadline: start + limits.total_timeout,
        idle: limits.idle_timeout,
        last_progress: Instant::now(),
        cancel,
        buf: vec![0u8; 16 << 10].into_boxed_slice(),
        pos: 0,
        len: 0,
    };

    // Status line: "HTTP/1.1 200 OK".
    let status_line = r.line()?;
    let s = std::str::from_utf8(&status_line).map_err(|_| HttpError::Malformed("status line"))?;
    let mut parts = s.splitn(3, ' ');
    let ver = parts.next().unwrap_or("");
    if !ver.starts_with("HTTP/1.") {
        return Err(HttpError::Malformed("not HTTP/1.x"));
    }
    let status: u16 = parts
        .next()
        .and_then(|c| c.parse().ok())
        .filter(|c| (100..600).contains(c))
        .ok_or(HttpError::Malformed("status code"))?;

    let mut headers = Vec::new();
    loop {
        let l = r.line()?;
        if l.is_empty() {
            break;
        }
        if headers.len() >= MAX_HEADERS {
            return Err(HttpError::Malformed("too many headers"));
        }
        let l = String::from_utf8(l).map_err(|_| HttpError::Malformed("header"))?;
        let (k, v) = l.split_once(':').ok_or(HttpError::Malformed("header"))?;
        headers.push((k.trim().to_owned(), v.trim().to_owned()));
    }

    let mut total = 0usize;
    let mut deliver = |data: &[u8]| -> Result<(), HttpError> {
        total += data.len();
        if total > limits.max_response_bytes {
            return Err(HttpError::ResponseTooLarge);
        }
        on_body(data)
    };

    let chunked = header(&headers, "transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked"));
    if chunked {
        loop {
            let size_line = r.line()?;
            let size_str = std::str::from_utf8(&size_line).map_err(|_| HttpError::Malformed("chunk size"))?;
            let size_hex = size_str.split(';').next().unwrap_or("").trim();
            let mut left = usize::from_str_radix(size_hex, 16).map_err(|_| HttpError::Malformed("chunk size"))?;
            if left == 0 {
                // Trailers until the empty line.
                while !r.line()?.is_empty() {}
                break;
            }
            if left > limits.max_response_bytes {
                return Err(HttpError::ResponseTooLarge);
            }
            while left > 0 {
                let c = r.chunk(left)?;
                if c.is_empty() {
                    return Err(HttpError::Malformed("eof in chunk"));
                }
                left -= c.len();
                let c = c.to_vec();
                deliver(&c)?;
            }
            if !r.line()?.is_empty() {
                return Err(HttpError::Malformed("chunk terminator"));
            }
        }
    } else if let Some(cl) = header(&headers, "content-length") {
        let mut left: usize = cl.trim().parse().map_err(|_| HttpError::Malformed("content-length"))?;
        if left > limits.max_response_bytes {
            return Err(HttpError::ResponseTooLarge);
        }
        while left > 0 {
            let c = r.chunk(left)?;
            if c.is_empty() {
                return Err(HttpError::Malformed("eof in body"));
            }
            left -= c.len();
            let c = c.to_vec();
            deliver(&c)?;
        }
    } else {
        loop {
            let c = r.chunk(usize::MAX)?;
            if c.is_empty() {
                break;
            }
            let c = c.to_vec();
            deliver(&c)?;
        }
    }
    Ok((status, headers))
}

/// Send a request and collect the whole response body.
pub fn send(
    addr: SocketAddr,
    req: &Request<'_>,
    limits: &Limits,
    cancel: Option<&CancelToken>,
) -> Result<Response, HttpError> {
    let mut body = Vec::new();
    let (status, headers) = send_stream(addr, req, limits, cancel, &mut |d| {
        body.extend_from_slice(d);
        Ok(())
    })?;
    Ok(Response { status, headers, body })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Serve exactly one connection with a canned response; returns the address and the request
    /// bytes the client sent (through the join handle).
    fn one_shot(resp: Vec<u8>, delay_ms: u64) -> (SocketAddr, std::thread::JoinHandle<Vec<u8>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let h = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
            let mut req = Vec::new();
            let mut b = [0u8; 4096];
            while let Ok(n) = s.read(&mut b) {
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&b[..n]);
                if req.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(delay_ms));
            let _ = s.write_all(&resp);
            req
        });
        (addr, h)
    }

    fn get(path: &str) -> Request<'_> {
        Request {
            method: "GET",
            path,
            headers: &[],
            body: None,
        }
    }

    #[test]
    fn content_length_body() {
        let (a, h) = one_shot(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec(), 0);
        let r = send(a, &get("/x"), &Limits::default(), None).unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, &b"hello"[..]));
        let req = String::from_utf8(h.join().unwrap()).unwrap();
        assert!(req.starts_with("GET /x HTTP/1.1\r\n") && req.contains("Connection: close"));
    }

    #[test]
    fn chunked_body_is_decoded() {
        let (a, _h) = one_shot(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;ext=1\r\nde\r\n0\r\n\r\n".to_vec(),
            0,
        );
        let r = send(a, &get("/"), &Limits::default(), None).unwrap();
        assert_eq!(r.body, b"abcde");
    }

    #[test]
    fn oversized_response_is_rejected() {
        let (a, _h) = one_shot(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n".to_vec(), 0);
        let l = Limits {
            max_response_bytes: 10,
            ..Limits::default()
        };
        assert_eq!(send(a, &get("/"), &l, None).unwrap_err(), HttpError::ResponseTooLarge);
    }

    #[test]
    fn oversized_request_is_rejected_before_connecting() {
        let l = Limits {
            max_request_bytes: 4,
            ..Limits::default()
        };
        let req = Request {
            method: "POST",
            path: "/",
            headers: &[],
            body: Some(b"12345"),
        };
        // Port 9 on loopback is never contacted: the size check comes first.
        let addr: SocketAddr = "127.0.0.1:9".parse().unwrap();
        assert_eq!(send(addr, &req, &l, None).unwrap_err(), HttpError::RequestTooLarge);
    }

    #[test]
    fn silent_server_times_out() {
        let (a, _h) = one_shot(b"HTTP/1.1 200 OK\r\n\r\n".to_vec(), 2_000);
        let l = Limits {
            idle_timeout: Duration::from_millis(200),
            ..Limits::default()
        };
        let t = Instant::now();
        assert_eq!(send(a, &get("/"), &l, None).unwrap_err(), HttpError::Timeout);
        assert!(t.elapsed() < Duration::from_millis(1_000));
    }

    #[test]
    fn cancellation_stops_waiting() {
        let (a, _h) = one_shot(b"HTTP/1.1 200 OK\r\n\r\n".to_vec(), 2_000);
        let c = CancelToken::new();
        let c2 = c.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            c2.cancel(CancelReason::Preempted);
        });
        let t = Instant::now();
        assert_eq!(
            send(a, &get("/"), &Limits::default(), Some(&c)).unwrap_err(),
            HttpError::Cancelled(CancelReason::Preempted)
        );
        assert!(t.elapsed() < Duration::from_millis(1_000));
    }

    #[test]
    fn header_injection_is_refused() {
        let req = Request {
            method: "GET",
            path: "/",
            headers: &[("X-A", "1\r\nX-Evil: 2")],
            body: None,
        };
        let addr: SocketAddr = "127.0.0.1:9".parse().unwrap();
        assert_eq!(
            send(addr, &req, &Limits::default(), None).unwrap_err(),
            HttpError::BadHeader
        );
    }

    #[test]
    fn garbage_status_line_is_malformed() {
        let (a, _h) = one_shot(b"SSH-2.0-OpenSSH\r\n\r\n".to_vec(), 0);
        assert!(matches!(
            send(a, &get("/"), &Limits::default(), None).unwrap_err(),
            HttpError::Malformed(_)
        ));
    }
}
