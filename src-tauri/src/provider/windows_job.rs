//! Own just this extractor's process tree; closing the job terminates its Deno children too.
use crate::error::{AppError, Result};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
    },
};

pub(super) struct ChildJob(HANDLE);
// SAFETY: a job is a thread-independent kernel handle, exclusively owned by this
// request. It can be moved across Tokio workers and closed on the final owner.
unsafe impl Send for ChildJob {}

impl ChildJob {
    pub fn attach(pid: Option<u32>) -> Result<Self> {
        let failed = || {
            AppError::new(
                "resolver_start",
                "Windows could not start an isolated source resolver. Try again.",
                true,
            )
        };
        let pid = pid.ok_or_else(failed)?;
        // SAFETY: all input structures live through synchronous calls. The PID
        // belongs to the child just spawned by this request, never a user-selected process.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(failed());
            }
            let job = Self(handle);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            ) == 0
            {
                return Err(failed());
            }
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if process.is_null() {
                return Err(failed());
            }
            let assigned = AssignProcessToJobObject(handle, process);
            CloseHandle(process);
            if assigned == 0 {
                return Err(failed());
            }
            Ok(job)
        }
    }
}

impl Drop for ChildJob {
    fn drop(&mut self) {
        // SAFETY: this object owns the only handle; KILL_ON_JOB_CLOSE applies only to its children.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_job_drop_terminates_its_own_child() {
        use std::{
            os::windows::process::CommandExt,
            process::Command,
            time::{Duration, Instant},
        };
        let mut child = Command::new("cmd.exe")
            .args(["/d", "/c", "ping -n 30 127.0.0.1 > nul"])
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        let job = ChildJob::attach(Some(child.id())).unwrap();
        drop(job);
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
