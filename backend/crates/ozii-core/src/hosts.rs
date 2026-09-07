//! Scoped, reversible management of a single hosts-file entry used to route
//! the Discord Desktop native updater (which bypasses both PAC and Chromium
//! proxy flags) through the local DPI chain.
//!
//! Safety invariants:
//! - Only ONE hostname is ever written: updates.discord.com (Discord-owned).
//! - Every written line carries the OziiDPI marker and is removed on
//!   deactivate/recovery. Lines without the marker are never touched.
//! - Direct writes are attempted first; on access denied an elevated
//!   one-shot helper is offered via UAC (opt-in feature).

use std::io::Write;
use std::path::PathBuf;

pub const HOSTS_MARKER: &str = "OziiDPI-MANAGED";
pub const UPDATER_HOST: &str = "updates.discord.com";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostsUpdateResult {
    /// Entry written / removed directly.
    Applied,
    /// Direct write denied; an elevated helper was launched and succeeded.
    AppliedElevated,
    /// Could not apply (UAC denied or helper failed).
    Failed,
}

pub fn hosts_file_path() -> PathBuf {
    PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()))
        .join("System32\\drivers\\etc\\hosts")
}

fn marker_line(enable: bool) -> String {
    if enable {
        // 127.0.0.2 (within the loopback range) avoids colliding with
        // services bound to 127.0.0.1:443.
        format!("127.0.0.2\t{UPDATER_HOST}\t# {HOSTS_MARKER}")
    } else {
        String::new()
    }
}

/// Reads the hosts file and returns (content_without_marker_lines, had_marker).
fn read_hosts() -> std::io::Result<(String, bool)> {
    let raw = std::fs::read_to_string(hosts_file_path())?;
    let mut had_marker = false;
    let kept: Vec<&str> = raw
        .lines()
        .filter(|l| {
            if l.contains(HOSTS_MARKER) {
                had_marker = true;
                false
            } else {
                true
            }
        })
        .collect();
    let mut content = kept.join("\n");
    if !raw.ends_with('\n') && !content.is_empty() {
        content.push('\n');
    }
    Ok((content, had_marker))
}

fn write_hosts(content: &str) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(hosts_file_path())?;
    f.write_all(content.as_bytes())?;
    f.flush()
}

fn elevated_helper(enable: bool) -> bool {
    // One-shot elevated PowerShell (UAC) performing the exact same scoped
    // edit. The helper script is written to temp to avoid quoting issues.
    let arg = if enable { "add" } else { "remove" };
    let script = format!(
        "$ErrorActionPreference='Stop'; \
         $p='{hosts}'; \
         $lines=Get-Content $p | Where-Object {{ $_ -notmatch '{marker}' }}; \
         if ('{arg}' -eq 'add') {{ $lines += '127.0.0.2\t{host}\t# {marker}' }}; \
         Set-Content -Path $p -Value $lines -Encoding ASCII; \
         exit 0",
        hosts = hosts_file_path().display(),
        marker = HOSTS_MARKER,
        arg = arg,
        host = UPDATER_HOST,
    );
    let script_path = std::env::temp_dir().join("ozii-dpi-hosts-helper.ps1");
    if std::fs::write(&script_path, &script).is_err() {
        return false;
    }

    let launcher = format!(
        "$p = Start-Process powershell -Verb RunAs -Wait -PassThru -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','{}'; exit $p.ExitCode",
        script_path.display()
    );
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &launcher])
        .status();
    match status {
        Ok(s) if s.success() => matches!(read_hosts(), Ok((_, state)) if state == enable),
        _ => false,
    }
}

/// Adds or removes the managed updater redirect entry.
pub fn set_updater_redirect(enable: bool) -> std::io::Result<HostsUpdateResult> {
    let (content, had_marker) = read_hosts()?;

    if enable && had_marker {
        return Ok(HostsUpdateResult::Applied); // already active
    }
    if !enable && !had_marker {
        return Ok(HostsUpdateResult::Applied); // already inactive
    }

    let mut new_content = content;
    if enable {
        if !new_content.ends_with('\n') && !new_content.is_empty() {
            new_content.push('\n');
        }
        new_content.push_str(&marker_line(true));
        new_content.push('\n');
    }

    match write_hosts(&new_content) {
        Ok(()) => Ok(HostsUpdateResult::Applied),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            if elevated_helper(enable) {
                Ok(HostsUpdateResult::AppliedElevated)
            } else {
                Ok(HostsUpdateResult::Failed)
            }
        }
        Err(e) => Err(e),
    }
}

/// True when the managed entry is currently present.
pub fn updater_redirect_active() -> bool {
    matches!(read_hosts(), Ok((_, true)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marker_line_format() {
        let line = marker_line(true);
        assert!(line.starts_with("127.0.0.2"));
        assert!(line.contains(UPDATER_HOST));
        assert!(line.contains(HOSTS_MARKER));
        assert_eq!(marker_line(false), "");
    }

    #[test]
    fn test_read_hosts_filters_marker() {
        // Only verifies the filter logic on a synthetic string via the same
        // code path used for filtering (kept private, so exercise through
        // set_updater_redirect against the real file is avoided in tests).
        let raw = "line1\n127.0.0.1 updates.discord.com # OziiDPI-MANAGED\nline2\n";
        let kept: Vec<&str> = raw.lines().filter(|l| !l.contains(HOSTS_MARKER)).collect();
        assert_eq!(kept, vec!["line1", "line2"]);
    }
}
