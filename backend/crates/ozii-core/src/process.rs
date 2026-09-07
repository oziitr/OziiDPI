#![allow(clippy::manual_map, clippy::collapsible_if)]
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub executable_path: Option<PathBuf>,
    pub start_time: u64, // UNIX timestamp in seconds
}

impl ProcessIdentity {
    /// Captures the identity of an active process by PID.
    pub fn capture(pid: u32) -> Option<Self> {
        let mut sys = System::new_all();
        sys.refresh_processes(ProcessesToUpdate::All, true);

        sys.process(Pid::from_u32(pid)).map(|process| Self {
            pid,
            executable_path: process.exe().map(|p| p.to_path_buf()),
            start_time: process.start_time(),
        })
    }

    /// Verifies if a given PID still represents THIS exact process execution.
    pub fn is_same_process(&self) -> bool {
        let mut sys = System::new_all();
        sys.refresh_processes(ProcessesToUpdate::All, true);

        if let Some(process) = sys.process(Pid::from_u32(self.pid)) {
            // Check if start time matches
            if process.start_time() != self.start_time {
                return false;
            }

            // Check if executable path matches
            if let Some(current_exe) = process.exe() {
                if self
                    .executable_path
                    .as_ref()
                    .is_some_and(|saved_exe| current_exe != saved_exe)
                {
                    return false;
                }
            }

            true
        } else {
            false
        }
    }

    /// Safely terminates the process ONLY if the identity matches.
    pub fn safe_kill(&self) -> Result<bool, String> {
        if self.is_same_process() {
            let mut sys = System::new_all();
            sys.refresh_processes(ProcessesToUpdate::All, true);
            if let Some(process) = sys.process(Pid::from_u32(self.pid)) {
                if process.kill() {
                    return Ok(true);
                } else {
                    return Err(format!("Failed to kill PID {}", self.pid));
                }
            }
        }
        // Process is already gone or identity mismatch (PID reused).
        // It's safe to assume the process we wanted to kill is not running.
        Ok(false)
    }
}
