//! Who is listening on a local TCP port (`GetExtendedTcpTable`).
//!
//! Used to authenticate the sidecar endpoint (plan §7.3): before trusting `127.0.0.1:<port>`, the
//! supervisor checks that the listening socket belongs to the process it started, so another
//! local program that grabbed the port in the bind/spawn race cannot pose as the model server.

use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};

use crate::WinError;

const AF_INET: u32 = 2;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

/// IPv4 listeners on `port`: (local address octets, owning pid).
pub fn ipv4_listeners(port: u16) -> Result<Vec<([u8; 4], u32)>, WinError> {
    let mut size = 0u32;
    // SAFETY: size query with a null buffer.
    let r = unsafe { GetExtendedTcpTable(None, &mut size, false, AF_INET, TCP_TABLE_OWNER_PID_LISTENER, 0) };
    if r != ERROR_INSUFFICIENT_BUFFER && r != 0 {
        return Err(WinError {
            op: "GetExtendedTcpTable",
            code: r as i32,
        });
    }
    // The table can grow between the two calls; retry a few times with headroom.
    for _ in 0..4 {
        let mut buf = vec![0u32; (size as usize / 4) + 64];
        let mut len = (buf.len() * 4) as u32;
        // SAFETY: buffer of `len` bytes, u32-aligned like MIB_TCPTABLE_OWNER_PID.
        let r = unsafe {
            GetExtendedTcpTable(
                Some(buf.as_mut_ptr() as *mut _),
                &mut len,
                false,
                AF_INET,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if r == ERROR_INSUFFICIENT_BUFFER {
            size = len;
            continue;
        }
        if r != 0 {
            return Err(WinError {
                op: "GetExtendedTcpTable",
                code: r as i32,
            });
        }
        let n = buf[0] as usize;
        let row_words = size_of::<MIB_TCPROW_OWNER_PID>() / 4;
        let mut out = Vec::new();
        for i in 0..n {
            let base = 1 + i * row_words;
            let Some(row) = buf.get(base..base + row_words) else {
                break;
            };
            // Row: state, local addr, local port, remote addr, remote port, pid.
            let local_port = u16::from_be((row[2] & 0xffff) as u16);
            if local_port == port {
                out.push((row[1].to_ne_bytes(), row[5]));
            }
        }
        return Ok(out);
    }
    Err(WinError {
        op: "GetExtendedTcpTable: table kept growing",
        code: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_own_listener_is_reported_with_our_pid() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let rows = ipv4_listeners(port).unwrap();
        assert_eq!(rows, vec![([127, 0, 0, 1], std::process::id())]);
        drop(l);
        assert!(ipv4_listeners(port).unwrap().is_empty());
    }
}
