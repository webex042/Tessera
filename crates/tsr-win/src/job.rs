//! Process containment with a kill-on-close Job Object (plan §4.1, §7.1).
//!
//! The sidecar is started **suspended**, assigned to a job created with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, and only then resumed — so it cannot run a single
//! instruction (or start a child of its own) outside the job. When the [`Job`] is dropped, or when
//! Tessara dies and Windows closes the handle for it, every process in the job is terminated.
//!
//! Resuming: `std::process::Command` does not expose the primary thread handle, so the threads of
//! the new process are found with a Toolhelp snapshot (documented API) and resumed. A suspended
//! process created by us has exactly one thread.

use std::os::windows::io::AsRawHandle;

use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Threading::{
    OpenProcess, OpenThread, PROCESS_SYNCHRONIZE, ResumeThread, THREAD_SUSPEND_RESUME, WaitForSingleObject,
};

use crate::WinError;

/// `CREATE_SUSPENDED | CREATE_NO_WINDOW`, for `CommandExt::creation_flags`.
pub const SPAWN_FLAGS: u32 = 0x0000_0004 | 0x0800_0000;

fn err(op: &'static str, e: windows::core::Error) -> WinError {
    WinError { op, code: e.code().0 }
}

/// A Job Object that kills its processes when the handle closes.
pub struct Job {
    handle: HANDLE,
}

// SAFETY: a job handle may be used and closed from any thread.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    pub fn new_kill_on_close() -> Result<Self, WinError> {
        // SAFETY: anonymous job, default security; the handle is owned by the returned value.
        let handle = unsafe { CreateJobObjectW(None, None) }.map_err(|e| err("CreateJobObjectW", e))?;
        let job = Job { handle };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // Kill everything on close; a crash inside the sidecar ends the process instead of
        // showing a Windows error dialog that would keep it (and its GPU memory) alive.
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
        // SAFETY: pointer and size describe `info`.
        unsafe {
            SetInformationJobObject(
                job.handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .map_err(|e| err("SetInformationJobObject", e))?;
        Ok(job)
    }

    /// Put a (suspended) child into the job.
    pub fn assign(&self, child: &std::process::Child) -> Result<(), WinError> {
        let h = HANDLE(child.as_raw_handle());
        // SAFETY: both handles are valid for the duration of the call.
        unsafe { AssignProcessToJobObject(self.handle, h) }.map_err(|e| err("AssignProcessToJobObject", e))
    }

    /// Whether `child` is in this job (used by tests and the diagnostics page).
    pub fn contains(&self, child: &std::process::Child) -> Result<bool, WinError> {
        let mut r = windows::core::BOOL(0);
        // SAFETY: out-param BOOL.
        unsafe { IsProcessInJob(HANDLE(child.as_raw_handle()), Some(self.handle), &mut r) }
            .map_err(|e| err("IsProcessInJob", e))?;
        Ok(r.as_bool())
    }

    /// Processes currently alive in the job (the sidecar plus anything it started).
    pub fn active_processes(&self) -> Result<u32, WinError> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: pointer and size describe `info`.
        unsafe {
            QueryInformationJobObject(
                Some(self.handle),
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut _,
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                None,
            )
        }
        .map_err(|e| err("QueryInformationJobObject", e))?;
        Ok(info.ActiveProcesses)
    }

    /// Process ids in the job (up to 256).
    pub fn process_ids(&self) -> Result<Vec<u32>, WinError> {
        // Header (two u32) followed by the id array, as the API lays it out.
        #[repr(C)]
        struct List {
            assigned: u32,
            in_list: u32,
            ids: [usize; 256],
        }
        let mut l = List {
            assigned: 0,
            in_list: 0,
            ids: [0; 256],
        };
        // SAFETY: pointer and size describe `l`, which matches JOBOBJECT_BASIC_PROCESS_ID_LIST.
        unsafe {
            QueryInformationJobObject(
                Some(self.handle),
                JobObjectBasicProcessIdList,
                &mut l as *mut _ as *mut _,
                size_of::<List>() as u32,
                None,
            )
        }
        .map_err(|e| err("QueryInformationJobObject", e))?;
        Ok(l.ids[..(l.in_list as usize).min(256)]
            .iter()
            .map(|&p| p as u32)
            .collect())
    }

    /// Kill every process in the job now (the handle stays open).
    pub fn terminate(&self, exit_code: u32) -> Result<(), WinError> {
        // SAFETY: valid job handle.
        unsafe { TerminateJobObject(self.handle, exit_code) }.map_err(|e| err("TerminateJobObject", e))
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: closing our own handle; with KILL_ON_JOB_CLOSE this ends every process in it.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Resume every thread of a process that was created with `CREATE_SUSPENDED`.
/// Returns how many threads were resumed (normally 1).
pub fn resume_process(pid: u32) -> Result<u32, WinError> {
    // SAFETY: snapshot handle is closed before returning; THREADENTRY32 has dwSize set.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).map_err(|e| err("CreateToolhelp32Snapshot", e))?;
        let mut te = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut resumed = 0u32;
        let mut ok = Thread32First(snap, &mut te).is_ok();
        let mut failure = None;
        while ok {
            if te.th32OwnerProcessID == pid {
                match OpenThread(THREAD_SUSPEND_RESUME, false, te.th32ThreadID) {
                    Ok(t) => {
                        if ResumeThread(t) == u32::MAX {
                            failure = Some(WinError {
                                op: "ResumeThread",
                                code: windows::core::Error::from_thread().code().0,
                            });
                        } else {
                            resumed += 1;
                        }
                        let _ = CloseHandle(t);
                    }
                    Err(e) => failure = Some(err("OpenThread", e)),
                }
            }
            ok = Thread32Next(snap, &mut te).is_ok();
        }
        let _ = CloseHandle(snap);
        match (resumed, failure) {
            (0, Some(e)) => Err(e),
            (0, None) => Err(WinError {
                op: "resume_process: no threads",
                code: 0,
            }),
            (n, _) => Ok(n),
        }
    }
}

/// Wait until process `pid` has exited. `true` if it is gone (or never existed) within `ms`.
pub fn wait_process_gone(pid: u32, ms: u32) -> bool {
    // SAFETY: synchronize-only handle, closed before returning.
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            return true; // no such process any more
        };
        let r = WaitForSingleObject(h, ms);
        let _ = CloseHandle(h);
        r == WAIT_OBJECT_0
    }
}
