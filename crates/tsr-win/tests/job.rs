//! REAL Job Object tests with disposable processes (`cmd.exe` running `ping` against loopback,
//! which gives a child *and* a grandchild). No user process is touched: every pid checked is one
//! the job itself reports.
#![cfg(windows)]

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tsr_win::job::{Job, SPAWN_FLAGS, resume_process, wait_process_gone};

fn spawn_suspended_pinger() -> std::process::Child {
    Command::new("cmd.exe")
        .args(["/c", "ping -n 120 127.0.0.1 > nul"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(SPAWN_FLAGS)
        .spawn()
        .expect("spawn cmd")
}

fn wait_for_processes(job: &Job, n: u32) -> Vec<u32> {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let ids = job.process_ids().unwrap();
        if ids.len() as u32 >= n || Instant::now() > end {
            return ids;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn suspended_child_runs_only_after_assignment_and_dies_with_the_job() {
    let job = Job::new_kill_on_close().unwrap();
    let mut child = spawn_suspended_pinger();
    // Still suspended: it cannot have started ping yet.
    std::thread::sleep(Duration::from_millis(300));
    job.assign(&child).unwrap();
    assert!(job.contains(&child).unwrap());
    assert_eq!(
        job.process_ids().unwrap(),
        vec![child.id()],
        "nothing ran before assignment"
    );
    assert_eq!(resume_process(child.id()).unwrap(), 1);

    // cmd starts ping; the grandchild is in the job too (jobs are inherited).
    let ids = wait_for_processes(&job, 2);
    println!("REAL job processes after resume: {}", ids.len());
    assert!(ids.len() >= 2, "grandchild not in job: {ids:?}");
    assert_eq!(job.active_processes().unwrap() as usize, ids.len());

    drop(job); // KILL_ON_JOB_CLOSE
    for pid in &ids {
        assert!(wait_process_gone(*pid, 5_000), "pid {pid} survived job close");
    }
    assert!(child.wait().is_ok());
}

#[test]
fn terminate_kills_everything_and_job_reports_empty() {
    let job = Job::new_kill_on_close().unwrap();
    let mut child = spawn_suspended_pinger();
    job.assign(&child).unwrap();
    resume_process(child.id()).unwrap();
    let ids = wait_for_processes(&job, 2);
    job.terminate(1).unwrap();
    for pid in &ids {
        assert!(wait_process_gone(*pid, 5_000));
    }
    assert_eq!(child.wait().unwrap().code(), Some(1));
    assert_eq!(job.active_processes().unwrap(), 0);
}

#[test]
fn resume_of_unknown_process_fails_cleanly() {
    // pid 0 is the idle process; it has no threads we may open, and nothing is resumed.
    assert!(resume_process(0).is_err());
}
