//! Starting one llama-server process (plan §7.1): command line, cleared environment, containment,
//! endpoint authentication and readiness. Restarts, health monitoring and idle unload are in
//! [`crate::supervisor`].
//!
//! Start sequence:
//! 1. Pick a free loopback port (bind `127.0.0.1:0`, release) and a fresh 32-byte API key.
//! 2. Spawn with an **empty environment** plus [`SIDECAR_ENV_ALLOWLIST`], no console window, stdout
//!    and stderr discarded (server logs can quote prompts), **suspended**.
//! 3. Windows: assign to a kill-on-close Job Object, then resume — the process never runs outside
//!    the job. Other OSes: the child is killed on drop (no grandchild containment; documented).
//! 4. Poll `GET /health` until ready, the process exits, or the ready timeout passes. Before the
//!    first health request is trusted, check that the listening socket belongs to our child
//!    (Windows), so a different program on that port cannot impersonate the sidecar.
//! 5. If the port was taken in the race (child exits early), retry with a new port, up to 3 times.

use std::ffi::OsString;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::backend::SIDECAR_ENV_ALLOWLIST;
use crate::cancel::{CancelReason, CancelToken};
use crate::client::{ClientError, Health, LoopbackClient};
use crate::select::Candidate;

/// Content-free launch failures.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, serde::Serialize)]
pub enum LaunchError {
    #[error("could not start the process: {0}")]
    Spawn(String),
    #[error("process containment failed: {0}")]
    Containment(String),
    #[error("process exited during startup (code {0:?})")]
    Exited(Option<i32>),
    #[error("not ready after {0} ms")]
    ReadyTimeout(u64),
    #[error("the endpoint rejected our API key")]
    Unauthorized,
    #[error("another process owns the sidecar port")]
    PortOwner,
    #[error("no free loopback port")]
    NoPort,
    #[error("cancelled")]
    Cancelled,
}

/// Everything needed to start the server. Paths are ours (models dir, pinned build), not content.
#[derive(Clone, Debug)]
pub struct SidecarSpec {
    pub server_exe: PathBuf,
    /// `None` starts llama-server in router mode (no model); only tests use that.
    pub model: Option<PathBuf>,
    pub mmproj: Option<PathBuf>,
    /// Device arguments from backend selection ([`Candidate::llama_args`]).
    pub device_args: Vec<String>,
    pub ctx: u32,
    pub threads: Option<u32>,
    /// Appended after the standard arguments (tests; future tuning flags).
    pub extra_args: Vec<String>,
    /// Added on top of the allow-list. Empty in production.
    pub extra_env: Vec<(String, OsString)>,
    pub ready_timeout: Duration,
}

impl SidecarSpec {
    pub fn from_candidate(
        c: &Candidate,
        model: PathBuf,
        mmproj: Option<PathBuf>,
        ctx: u32,
        threads: Option<u32>,
    ) -> Self {
        SidecarSpec {
            server_exe: c.server_exe.clone(),
            model: Some(model),
            mmproj,
            device_args: c.llama_args(),
            ctx,
            threads,
            extra_args: Vec::new(),
            extra_env: Vec::new(),
            ready_timeout: Duration::from_secs(90),
        }
    }
}

/// The plan §7.1 command line (checked against b11900 `--help`, docs/stage0).
pub fn server_args(spec: &SidecarSpec, port: u16, api_key: &str) -> Vec<String> {
    let mut a: Vec<String> = vec!["--host".into(), "127.0.0.1".into(), "--port".into(), port.to_string()];
    a.extend(["--api-key".into(), api_key.to_owned()]);
    if let Some(m) = &spec.model {
        a.extend(["-m".into(), m.display().to_string()]);
    }
    if let Some(m) = &spec.mmproj {
        a.extend(["--mmproj".into(), m.display().to_string()]);
    }
    a.extend(spec.device_args.iter().cloned());
    a.extend(
        [
            "-c",
            &spec.ctx.to_string(),
            "-np",
            "1",
            "-fa",
            "on",
            "-ctk",
            "q8_0",
            "-ctv",
            "q8_0",
            "--jinja",
            // Attack surface off: web UI, agent tools, MCP proxy, slot dump, credentialed CORS.
            "--no-webui",
            "--offline",
            "--no-agent",
            "--no-ui-mcp-proxy",
            "--no-slots",
            "--no-cors-credentials",
            "--cors-origins",
            "http://tessara.invalid",
            // Fail loudly instead of silently shrinking; no host-RAM prompt cache.
            "--fit",
            "off",
            "--cache-ram",
            "0",
            "--reasoning",
            "off",
        ]
        .map(String::from),
    );
    if let Some(t) = spec.threads {
        a.extend(["-t".into(), t.to_string()]);
    }
    a.extend(spec.extra_args.iter().cloned());
    a
}

/// The child's whole environment: the allow-list (values from our own environment) plus
/// `extra_env`. Nothing else — `LLAMA_ARG_*`, `HF_TOKEN`, `GGML_*`, proxies — reaches it.
pub fn sidecar_env(extra: &[(String, OsString)]) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = SIDECAR_ENV_ALLOWLIST
        .iter()
        .filter_map(|k| std::env::var_os(k).map(|v| (OsString::from(k), v)))
        .collect();
    env.extend(extra.iter().map(|(k, v)| (OsString::from(k), v.clone())));
    env
}

pub fn random_api_key() -> Zeroizing<String> {
    #[cfg(windows)]
    let bytes = tsr_win::dpapi::random_bytes::<32>().expect("BCryptGenRandom");
    #[cfg(not(windows))]
    let bytes = {
        use std::io::Read;
        let mut b = Zeroizing::new([0u8; 32]);
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut b[..]))
            .expect("/dev/urandom");
        b
    };
    let mut s = Zeroizing::new(String::with_capacity(64));
    for b in bytes.iter() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub fn free_loopback_port() -> Result<u16, LaunchError> {
    let l = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).map_err(|_| LaunchError::NoPort)?;
    l.local_addr().map(|a| a.port()).map_err(|_| LaunchError::NoPort)
}

/// Platform containment for one child (and, on Windows, everything it starts).
pub struct Containment {
    #[cfg(windows)]
    job: tsr_win::job::Job,
}

impl Containment {
    /// Processes still alive inside (Windows); `None` where the platform cannot tell.
    pub fn process_count(&self) -> Option<u32> {
        #[cfg(windows)]
        {
            self.job.active_processes().ok()
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    pub fn process_ids(&self) -> Vec<u32> {
        #[cfg(windows)]
        {
            self.job.process_ids().unwrap_or_default()
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }

    fn kill_all(&self) {
        #[cfg(windows)]
        {
            let _ = self.job.terminate(1);
        }
    }
}

/// Spawn `cmd` so that it starts executing only once it is contained.
pub fn spawn_contained(cmd: &mut Command) -> Result<(Child, Containment), LaunchError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use tsr_win::job::{Job, SPAWN_FLAGS, resume_process};
        let job = Job::new_kill_on_close().map_err(|e| LaunchError::Containment(e.to_string()))?;
        cmd.creation_flags(SPAWN_FLAGS);
        let mut child = cmd.spawn().map_err(|e| LaunchError::Spawn(format!("{:?}", e.kind())))?;
        if let Err(e) = job.assign(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(LaunchError::Containment(e.to_string()));
        }
        if let Err(e) = resume_process(child.id()) {
            let _ = job.terminate(1);
            let _ = child.wait();
            return Err(LaunchError::Containment(e.to_string()));
        }
        Ok((child, Containment { job }))
    }
    #[cfg(not(windows))]
    {
        let child = cmd.spawn().map_err(|e| LaunchError::Spawn(format!("{:?}", e.kind())))?;
        Ok((child, Containment {}))
    }
}

/// A started, ready sidecar. Dropping it kills the process tree.
pub struct SidecarProcess {
    child: Child,
    containment: Containment,
    client: LoopbackClient,
    pub started_at: Instant,
    pub ready_ms: u64,
}

impl std::fmt::Debug for SidecarProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SidecarProcess")
            .field("pid", &self.child.id())
            .field("addr", &self.client.addr())
            .finish()
    }
}

impl SidecarProcess {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn client(&self) -> &LoopbackClient {
        &self.client
    }

    pub fn containment(&self) -> &Containment {
        &self.containment
    }

    /// `Some(code)` once the process has exited.
    pub fn exited(&mut self) -> Option<Option<i32>> {
        match self.child.try_wait() {
            Ok(Some(st)) => Some(st.code()),
            Ok(None) => None,
            Err(_) => Some(None),
        }
    }

    /// Kill the process and everything it started; waits for the child to be reaped.
    pub fn kill(&mut self) {
        self.containment.kill_all();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for SidecarProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

/// On Windows, the listening socket for `port` must belong to `pid`.
fn endpoint_owned_by(port: u16, pid: u32) -> Option<bool> {
    #[cfg(windows)]
    {
        let rows = tsr_win::tcp::ipv4_listeners(port).ok()?;
        if rows.is_empty() {
            return None; // not listening yet
        }
        Some(rows.iter().all(|(addr, p)| *p == pid && *addr == [127, 0, 0, 1]))
    }
    #[cfg(not(windows))]
    {
        let _ = (port, pid);
        Some(true)
    }
}

fn start_once(spec: &SidecarSpec, cancel: &CancelToken) -> Result<SidecarProcess, LaunchError> {
    start_on_port(spec, free_loopback_port()?, cancel)
}

/// One start attempt on a given port. [`start`] picks the port; this is public so tests can
/// reproduce the bind race (another program already listening there).
pub fn start_on_port(spec: &SidecarSpec, port: u16, cancel: &CancelToken) -> Result<SidecarProcess, LaunchError> {
    let key = random_api_key();
    let mut cmd = Command::new(&spec.server_exe);
    cmd.args(server_args(spec, port, &key))
        .env_clear()
        .envs(sidecar_env(&spec.extra_env))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = spec.server_exe.parent() {
        cmd.current_dir(dir);
    }
    let started_at = Instant::now();
    let (child, containment) = spawn_contained(&mut cmd)?;
    let client =
        LoopbackClient::new(SocketAddr::from((Ipv4Addr::LOCALHOST, port)), &key).map_err(|_| LaunchError::NoPort)?;
    let mut p = SidecarProcess {
        child,
        containment,
        client,
        started_at,
        ready_ms: 0,
    };
    let mut owner_checked = false;
    loop {
        if cancel.is_cancelled() {
            return Err(LaunchError::Cancelled);
        }
        if let Some(code) = p.exited() {
            return Err(LaunchError::Exited(code));
        }
        if started_at.elapsed() > spec.ready_timeout {
            return Err(LaunchError::ReadyTimeout(spec.ready_timeout.as_millis() as u64));
        }
        if !owner_checked {
            match endpoint_owned_by(port, p.pid()) {
                None => {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Some(false) => return Err(LaunchError::PortOwner),
                Some(true) => owner_checked = true,
            }
        }
        match p.client.health(Duration::from_secs(2)) {
            Ok(Health::Ready) => {
                p.ready_ms = started_at.elapsed().as_millis() as u64;
                return Ok(p);
            }
            Ok(Health::Loading) => {}
            Err(ClientError::Unauthorized) => return Err(LaunchError::Unauthorized),
            Err(_) => {} // not accepting yet
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Start the sidecar, retrying with a new port if the first one was lost in the bind race.
pub fn start(spec: &SidecarSpec, cancel: &CancelToken) -> Result<SidecarProcess, LaunchError> {
    let mut last = LaunchError::NoPort;
    for _ in 0..3 {
        match start_once(spec, cancel) {
            Ok(p) => return Ok(p),
            // An early exit is what a lost port race looks like; anything else will not improve.
            Err(e @ (LaunchError::Exited(_) | LaunchError::PortOwner)) => last = e,
            Err(e) => return Err(e),
        }
        if cancel.reason() == Some(CancelReason::Shutdown) {
            return Err(LaunchError::Cancelled);
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SidecarSpec {
        SidecarSpec {
            server_exe: PathBuf::from("llama-server.exe"),
            model: Some(PathBuf::from("m.gguf")),
            mmproj: Some(PathBuf::from("mm.gguf")),
            device_args: vec!["--device".into(), "Vulkan0".into(), "-ngl".into(), "all".into()],
            ctx: 8192,
            threads: Some(4),
            extra_args: vec![],
            extra_env: vec![],
            ready_timeout: Duration::from_secs(1),
        }
    }

    #[test]
    fn command_line_has_every_hardening_flag_and_binds_loopback() {
        let a = server_args(&spec(), 4321, "k");
        let joined = a.join(" ");
        for f in [
            "--host 127.0.0.1",
            "--port 4321",
            "--api-key k",
            "--offline",
            "--no-webui",
            "--no-agent",
            "--no-ui-mcp-proxy",
            "--no-slots",
            "--no-cors-credentials",
            "--cors-origins http://tessara.invalid",
            "--fit off",
            "--cache-ram 0",
            "-np 1",
            "-c 8192",
            "--device Vulkan0 -ngl all",
            "-m m.gguf",
            "--mmproj mm.gguf",
        ] {
            assert!(joined.contains(f), "missing {f}");
        }
        assert!(!joined.contains("0.0.0.0"));
    }

    #[test]
    fn child_environment_is_only_the_allow_list() {
        // The test process may have any variables; the child gets the allow-list only.
        let env = sidecar_env(&[]);
        for (k, _) in &env {
            assert!(
                SIDECAR_ENV_ALLOWLIST
                    .iter()
                    .any(|a| std::ffi::OsStr::new(a) == k.as_os_str()),
                "{k:?}"
            );
        }
        let extra = sidecar_env(&[("LLAMA_CACHE".into(), "x".into())]);
        assert_eq!(extra.len(), env.len() + 1);
    }

    #[test]
    fn api_keys_are_random_hex() {
        let a = random_api_key();
        let b = random_api_key();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(*a, *b);
    }

    #[test]
    fn missing_executable_is_a_spawn_error() {
        let mut s = spec();
        s.server_exe = PathBuf::from("Z:/definitely/not/here/llama-server.exe");
        assert!(matches!(start(&s, &CancelToken::new()), Err(LaunchError::Spawn(_))));
    }
}
