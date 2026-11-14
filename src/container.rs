use anyhow::{bail, Result};
use std::path::PathBuf;

use crate::constants::*;
use crate::utils::*;

/// sh snippet: exec the magisk CLI with MAGISKTMP pinned to the container's
/// /sbin, forwarding "$@" untouched.
fn magisk_exec_script() -> String {
    format!("MAGISKTMP={CONTAINER_MAGISK_TMP} exec {CONTAINER_MAGISK_BIN} \"$@\"")
}

/// Build the argv for `waydroid shell ...`. waydroid's argparse treats any
/// container-side argument starting with '-' as one of its own options (the
/// shell subparser even defines real -u/-g/-s/-L/-C/-G flags), so `waydroid
/// shell sh -ec ...` dies with "unrecognized arguments". A `--` separator
/// makes argparse hand everything after it to COMMAND, which upstream
/// forwards verbatim to `lxc-attach ... --` (verified against 1.4.2 and
/// master/1.6.x).
fn shell_cmd<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let mut all: Vec<&'a str> = vec!["shell", "--"];
    all.extend_from_slice(args);
    all
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
    pub fn shell(&self, args: &[&str]) -> Result<String> {
        self.require()?;
        self.require_running()?;
        run_out("waydroid", &shell_cmd(args))
    }

    pub fn shell_try(&self, args: &[&str]) -> (bool, String) {
        run_try("waydroid", &shell_cmd(args))
    }

    /// Run a daemon CLI command with MAGISKTMP pinned to /sbin.
    /// Args are passed as separate argv entries and re-expanded via "$@" so
    /// SQL statements and paths survive without quoting problems.
    pub fn magisk_cli(&self, args: &[&str]) -> Result<String> {
        self.require()?;
        self.require_running()?;
        let script = magisk_exec_script();
        let mut all: Vec<&str> = vec!["shell", "--", "sh", "-ec", &script, "magisk"];
        all.extend_from_slice(args);
        run_out("waydroid", &all)
    }

    pub fn magisk_cli_try(&self, args: &[&str]) -> (bool, String) {
        let script = magisk_exec_script();
        let mut all: Vec<&str> = vec!["shell", "--", "sh", "-ec", &script, "magisk"];
        all.extend_from_slice(args);
        // NOTE: run_try directly — calling shell_try here would prepend a
        // second "shell" token (it already builds `waydroid shell -- ...`).
        run_try("waydroid", &all)
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
