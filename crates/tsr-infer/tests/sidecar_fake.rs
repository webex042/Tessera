//! Sidecar start/stop against the SIMULATED server `tsr-fake-llama` — a real separate process
//! started by the real sidecar code (containment, cleared environment, endpoint check, health).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tsr_core::Sensitive;
use tsr_infer::cancel::{CancelReason, CancelToken};
use tsr_infer::client::{ChatModel, ChatRequest, ClientError, Health, LoopbackClient};
use tsr_infer::sidecar::{LaunchError, SidecarSpec, start};

fn fake_spec(extra: &[&str]) -> SidecarSpec {
    SidecarSpec {
        server_exe: PathBuf::from(env!("CARGO_BIN_EXE_tsr-fake-llama")),
        model: None,
        mmproj: None,
        device_args: vec!["--device".into(), "none".into(), "-ngl".into(), "0".into()],
        ctx: 2048,
        threads: Some(1),
        extra_args: extra.iter().map(|s| s.to_string()).collect(),
        extra_env: vec![],
        ready_timeout: Duration::from_secs(10),
    }
}

fn req(stream: bool) -> ChatRequest {
    ChatRequest {
        system: Sensitive::new("sys".into()),
        user: Sensitive::new("hello".into()),
        image_png: None,
        temperature: 0.0,
        max_tokens: 16,
        json_schema: None,
        stream,
        seed: Some(1),
    }
}

#[cfg(windows)]
fn gone(pid: u32) -> bool {
    tsr_win::job::wait_process_gone(pid, 5_000)
}

#[test]
fn starts_contained_answers_and_dies_on_drop() {
    let p = start(&fake_spec(&[]), &CancelToken::new()).expect("start");
    let pid = p.pid();
    #[cfg(windows)]
    {
        // Windows adds a conhost.exe for a console program even with CREATE_NO_WINDOW; it is in
        // the job as well and dies with it.
        let ids = p.containment().process_ids();
        assert!(ids.contains(&pid), "sidecar is in its job: {ids:?}");
        for other in ids.iter().filter(|&&i| i != pid) {
            let exe = tsr_win::query::process_image(*other).unwrap_or_default().to_lowercase();
            assert!(exe.ends_with("conhost.exe"), "unexpected process in job: {exe}");
        }
    }
    assert_eq!(p.client().health(Duration::from_secs(1)).unwrap(), Health::Ready);
    for stream in [false, true] {
        let r = p.client().chat(&req(stream), &CancelToken::new()).unwrap();
        assert_eq!(r.content.expose(), "{\"ok\":true}", "stream={stream}");
        assert_eq!(r.finish_reason.as_deref(), Some("stop"));
    }
    // A client with another key is refused by the server.
    let wrong = LoopbackClient::new(p.client().addr(), "not-the-key").unwrap();
    assert_eq!(
        wrong.chat(&req(false), &CancelToken::new()).unwrap_err(),
        ClientError::Unauthorized
    );
    drop(p);
    #[cfg(windows)]
    assert!(gone(pid));
}

#[test]
fn waits_while_loading() {
    let p = start(&fake_spec(&["--fake-load-ms", "700"]), &CancelToken::new()).unwrap();
    assert!(p.ready_ms >= 700, "{}", p.ready_ms);
}

#[test]
fn ready_timeout_kills_the_process() {
    let mut s = fake_spec(&["--fake-load-ms", "60000"]);
    s.ready_timeout = Duration::from_millis(800);
    let t = Instant::now();
    assert_eq!(
        start(&s, &CancelToken::new()).unwrap_err(),
        LaunchError::ReadyTimeout(800)
    );
    assert!(t.elapsed() < Duration::from_secs(5));
}

#[test]
fn crash_on_start_is_reported_after_port_retries() {
    assert_eq!(
        start(&fake_spec(&["--fake-crash-on-start"]), &CancelToken::new()).unwrap_err(),
        LaunchError::Exited(Some(5))
    );
}

#[test]
fn cancelled_start_stops_waiting() {
    let c = CancelToken::new();
    let c2 = c.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        c2.cancel(CancelReason::Shutdown);
    });
    let t = Instant::now();
    assert_eq!(
        start(&fake_spec(&["--fake-load-ms", "60000"]), &c).unwrap_err(),
        LaunchError::Cancelled
    );
    assert!(t.elapsed() < Duration::from_secs(3));
}

#[test]
fn streaming_request_can_be_dropped_mid_generation() {
    let p = start(
        &fake_spec(&["--fake-token-ms", "50", "--fake-tokens", "40"]),
        &CancelToken::new(),
    )
    .unwrap();
    let c = CancelToken::new();
    let c2 = c.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        c2.cancel(CancelReason::Preempted);
    });
    let t = Instant::now();
    let e = p.client().chat(&req(true), &c).unwrap_err();
    assert_eq!(
        e,
        ClientError::Http(tsr_infer::http::HttpError::Cancelled(CancelReason::Preempted))
    );
    assert!(
        t.elapsed() < Duration::from_millis(1_500),
        "stopped early, not after the 2 s reply"
    );
}

#[cfg(windows)]
#[test]
fn grandchildren_die_with_the_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("grandchild.pid");
    let p = start(
        &fake_spec(&["--fake-spawn-child", pidfile.to_str().unwrap()]),
        &CancelToken::new(),
    )
    .unwrap();
    let gc: u32 = std::fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
    let ids = p.containment().process_ids();
    assert!(ids.contains(&gc), "grandchild {gc} inside the job: {ids:?}");
    let pid = p.pid();
    drop(p);
    assert!(gone(pid) && gone(gc), "sidecar and grandchild terminated");
}

/// An impostor already listening on the port must not be mistaken for the sidecar, even though
/// it answers `/health` like one.
#[cfg(windows)]
#[test]
fn impostor_on_the_port_is_refused() {
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let h = std::thread::spawn(move || {
        l.set_nonblocking(true).unwrap();
        let mut served = 0;
        while !s2.load(Ordering::Relaxed) {
            if let Ok((mut c, _)) = l.accept() {
                c.set_nonblocking(false).unwrap();
                let mut b = [0u8; 1024];
                let _ = c.read(&mut b);
                let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\n\r\n{\"status\":\"ok\"}");
                served += 1;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        served
    });
    let r = tsr_infer::sidecar::start_on_port(&fake_spec(&[]), port, &CancelToken::new());
    stop.store(true, Ordering::Relaxed);
    let served = h.join().unwrap();
    match r {
        Err(LaunchError::PortOwner | LaunchError::Exited(_)) => {}
        other => panic!("impostor accepted: {other:?}"),
    }
    assert_eq!(served, 0, "no request was sent to the impostor");
}
