use anyhow::{bail, Result};
use std::path::PathBuf;

use crate::constants::*;
use crate::utils::*;

/// sh script wrapper for generic container commands. `waydroid shell` execs
/// lxc-attach through subprocess.run() and DISCARDS the child exit status
/// (waydroid tools/helpers/lxc.py), so success/failure must be detected by
/// parsing an explicit marker line the script prints after the command.
const SHELL_RC_SCRIPT: &str = "\"$@\"\n__wsu_rc=$?\necho \"__WSU_RC=$__wsu_rc\"";

/// sh script wrapper for daemon CLI commands. The runtime dir is autodetected
/// with the same rule as KitsuneMagisk live_setup.sh: /sbin when the image has
/// one, /debug_ramdisk otherwise (Q+ images ship no /sbin). Args are passed as
/// separate argv entries and re-expanded via "$@" so SQL statements and paths
/// survive without quoting problems.
const MAGISK_RC_SCRIPT: &str = "\
if [ -x /sbin/magisk ]; then
  __wsu_mt=/sbin
elif [ -x /debug_ramdisk/magisk ]; then
  __wsu_mt=/debug_ramdisk
elif [ -d /sbin ] && [ ! -L /sbin ]; then
  __wsu_mt=/sbin
else
  __wsu_mt=/debug_ramdisk
fi
if [ ! -x \"$__wsu_mt/magisk\" ]; then
  echo \"magisk binary not found in container (checked /sbin/magisk, /debug_ramdisk/magisk)\"
  echo \"__WSU_RC=127\"
else
  MAGISKTMP=\"$__wsu_mt\" \"$__wsu_mt/magisk\" \"$@\"
  echo \"__WSU_RC=$?\"
fi";

/// Strip the exit-code marker from container output; returns (ok, output).
/// Without a marker the waydroid layer failed outright (or output was empty).
fn parse_rc_out(text: &str) -> (bool, String) {
    let mut rc: Option<i64> = None;
    let mut out = String::new();
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("__WSU_RC=") {
            rc = v.trim().parse::<i64>().ok();
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    match rc {
        Some(r) => (r == 0, out.trim().to_string()),
        None => (false, text.trim().to_string()),
    }
}

/// Minimal waydroid container interface.
pub struct Container {
    pub present: bool,
}

impl Container {
    pub fn detect() -> Self {
        let (ok, _) = run_try("sh", &["-c", "command -v waydroid >/dev/null 2>&1"]);
        let dir = PathBuf::from(WAYDROID_DIR).exists();
        Self { present: ok && dir }
    }

    pub fn require(&self) -> Result<()> {
        if !self.present {
            bail!("Waydroid is not installed or /var/lib/waydroid is missing (install waydroid first)");
        }
        Ok(())
    }

    /// `waydroid status` reports no usable container (STOPPED, or waydroid
    /// not initialized). FROZEN counts as usable — `waydroid shell` unfreezes.
    pub fn container_stopped(&self) -> bool {
        match self
            .status_text()
            .lines()
            .find(|l| l.starts_with("Container:"))
        {
            Some(l) => !l.contains("RUNNING") && !l.contains("FROZEN"),
            None => true,
        }
    }

    fn require_running(&self) -> Result<()> {
        if self.container_stopped() {
            bail!("Waydroid container is not running — start it first (e.g. `waydroid show-full-ui` or `sudo wsu restart`)");
        }
        Ok(())
    }

    /// `waydroid shell <args...>` — runs inside the container as root.
    /// waydroid's argparse treats any container-side argument starting with
    /// '-' as one of its own options (the shell subparser even defines real
    /// -u/-g/-s/-L/-C/-G flags), so a `--` separator is always inserted:
    /// everything after it reaches lxc-attach verbatim (verified against
    /// waydroid 1.4.2 and master/1.6.x).
    pub fn shell(&self, args: &[&str]) -> Result<String> {
        self.require()?;
        self.require_running()?;
        let (ok, out) = self.shell_try(args);
        if !ok {
            bail!("container command failed: {}", if out.is_empty() { "no output".into() } else { out });
        }
        Ok(out)
    }

    pub fn shell_try(&self, args: &[&str]) -> (bool, String) {
        let mut all: Vec<&str> = vec!["shell", "--", "sh", "-c", SHELL_RC_SCRIPT, "wsu"];
        all.extend_from_slice(args);
        let (_, out) = run_try("waydroid", &all);
        parse_rc_out(&out)
    }

    /// Run a daemon CLI command with MAGISKTMP autodetected inside the
    /// container. Fails when the magisk binary is absent or the command exits
    /// non-zero (detected via the rc marker, since waydroid swallows exit
    /// codes).
    pub fn magisk_cli(&self, args: &[&str]) -> Result<String> {
        self.require()?;
        self.require_running()?;
        let (ok, out) = self.magisk_cli_try(args);
        if !ok {
            bail!("{}", if out.is_empty() { "magisk command failed".to_string() } else { out });
        }
        Ok(out)
    }

    pub fn magisk_cli_try(&self, args: &[&str]) -> (bool, String) {
        let mut all: Vec<&str> = vec!["shell", "--", "sh", "-c", MAGISK_RC_SCRIPT, "magisk"];
        all.extend_from_slice(args);
        let (_, out) = run_try("waydroid", &all);
        parse_rc_out(&out)
    }

    /// Raw container status output from `waydroid status`.
    pub fn status_text(&self) -> String {
        let (_, out) = run_try("waydroid", &["status"]);
        out
    }

    pub fn container_running(&self) -> bool {
        self.status_text().contains("Container:")
            && self
                .status_text()
                .lines()
                .any(|l| l.starts_with("Container:") && l.contains("RUNNING"))
    }

    pub fn session_running(&self) -> bool {
        self.status_text()
            .lines()
            .any(|l| l.starts_with("Session:") && l.contains("RUNNING"))
    }

    pub fn stop_all(&self) -> Result<()> {
        if self.session_running() {
            msg_sub("stopping waydroid session");
            let _ = run_try("waydroid", &["session", "stop"]);
        }
        if self.container_running() {
            msg_sub("stopping waydroid container");
            let _ = run_try("waydroid", &["container", "stop"]);
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        Ok(())
    }

    pub fn start_container(&self) -> Result<()> {
        self.require()?;
        if self.container_running() {
            return Ok(());
        }
        msg_sub("starting waydroid container");
        let (ok, err) = run_try("waydroid", &["container", "start"]);
        if !ok {
            msg_warn(&format!("`waydroid container start` did not return cleanly: {err}"));
        }
        Ok(())
    }

    /// Is `mount_overlays` enabled in waydroid.cfg? (default true)
    pub fn overlays_enabled(&self) -> bool {
        match std::fs::read_to_string(WAYDROID_CFG) {
            Ok(cfg) => match cfg_lookup(&cfg, "waydroid", "mount_overlays") {
                Some(v) => v.eq_ignore_ascii_case("true") || v == "1",
                None => true,
            },
            Err(_) => true,
        }
    }

    /// Resolve the host-side writable "system root" the installer writes into.
    /// Preference: overlay/system (when overlays enabled and present) ->
    /// rootfs/system (live rootfs dir) -> mounted image via `waydroid mount`.
    /// Returns (system_root_path, kind) where kind ∈ {"overlay","rootfs","image"}.
    pub fn system_root(&self) -> Result<(PathBuf, &'static str)> {
        let overlay = PathBuf::from(OVERLAY_SYSTEM);
        let rootfs = PathBuf::from(ROOTFS_SYSTEM);
        if self.overlays_enabled() {
            std::fs::create_dir_all(&overlay).ok();
            return Ok((overlay, "overlay"));
        }
        if rootfs.is_dir() {
            return Ok((rootfs, "rootfs"));
        }
        // Last resort: mount the system image read-write at /mnt/waydroid.
        let (ok, _) = run_try("waydroid", &["mount"]);
        let mounted = PathBuf::from("/mnt/waydroid/system");
        if ok && mounted.is_dir() {
            return Ok((PathBuf::from("/mnt/waydroid"), "image"));
        }
        bail!("could not resolve a writable waydroid system root (tried overlay, rootfs, waydroid mount)");
    }

    pub fn magisk_dir_host(&self) -> Result<PathBuf> {
        let (sys, _) = self.system_root()?;
        Ok(sys.join(MAGISK_DIR_REL))
    }

    pub fn is_installed(&self) -> bool {
        match self.magisk_dir_host() {
            Ok(d) => d.join("magisk").is_file(),
            Err(_) => false,
        }
    }
}

/// Parse waydroid.cfg for a key in a section.
pub fn cfg_lookup(cfg: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in cfg.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_section = line[1..line.len() - 1].trim().eq_ignore_ascii_case(section);
            continue;
        }
        if in_section {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rc_out_success() {
        let (ok, out) = parse_rc_out("package:foo uid:10100\n__WSU_RC=0\n");
        assert!(ok);
        assert_eq!(out, "package:foo uid:10100");
    }

    #[test]
    fn test_parse_rc_out_failure() {
        let (ok, out) = parse_rc_out("pm: not found\n__WSU_RC=1\n");
        assert!(!ok);
        assert_eq!(out, "pm: not found");
    }

    #[test]
    fn test_parse_rc_out_missing_marker_means_failed_layer() {
        // waydroid itself failed (no marker printed) — must not read as success
        let (ok, _) = parse_rc_out("some stale output");
        assert!(!ok);
    }

    #[test]
    fn test_parse_rc_out_marker_in_error_text() {
        let (ok, out) = parse_rc_out("magisk binary not found in container\n__WSU_RC=127\n");
        assert!(!ok);
        assert!(out.contains("not found"));
    }

    /// The two wrapper scripts must be parseable POSIX sh.
    #[test]
    fn test_wrapper_scripts_syntax() {
        for s in [SHELL_RC_SCRIPT, MAGISK_RC_SCRIPT, crate::constants::WSU_BOOT_SH] {
            let (ok, err) = run_try("sh", &["-n", "-c", s]);
            assert!(ok, "sh -n rejected script: {err}");
        }
    }
}
