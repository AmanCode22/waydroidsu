use anyhow::{bail, Context, Result};
use colored::Colorize;

use crate::constants::*;
use crate::container::Container;
use crate::magisk::install_manager_apk;
use crate::utils::*;

/// One su policy row from magisk.db (policies table).
#[derive(Debug, Clone)]
pub struct Policy {
    pub uid: i64,
    pub policy: i64,
    pub until: i64,
    pub logging: i64,
    pub notification: i64,
}

fn parse_rows(out: &str) -> Vec<Policy> {
    let mut rows = Vec::new();
    for line in out.lines() {
        if line.trim().is_empty() || !line.contains('=') {
            continue;
        }
        let mut p = Policy { uid: -1, policy: -1, until: 0, logging: 1, notification: 1 };
        for kv in line.split('|') {
            if let Some((k, v)) = kv.split_once('=') {
                match k {
                    "uid" => p.uid = v.parse().unwrap_or(-1),
                    "policy" => p.policy = v.parse().unwrap_or(-1),
                    "until" => p.until = v.parse().unwrap_or(0),
                    "logging" => p.logging = v.parse().unwrap_or(1),
                    "notification" => p.notification = v.parse().unwrap_or(1),
                    _ => {}
                }
            }
        }
        if p.uid >= 0 {
            rows.push(p);
        }
    }
    rows
}

/// uid -> package map from `pm list packages -U` (uid and app id both tried).
fn package_map(c: &Container) -> Result<Vec<(i64, String)>> {
    let out = c.shell(&["pm", "list", "packages", "-U"])?;
    let mut map = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if !line.starts_with("package:") {
            continue;
        }
        let mut pkg = String::new();
        let mut uid = -1i64;
        for tok in line.split_whitespace() {
            if let Some(p) = tok.strip_prefix("package:") {
                pkg = p.to_string();
            } else if let Some(u) = tok.strip_prefix("uid:") {
                uid = u.parse().unwrap_or(-1);
            }
        }
        if !pkg.is_empty() && uid >= 0 {
            map.push((uid, pkg));
        }
    }
    Ok(map)
}

pub fn list(c: &Container) -> Result<()> {
    let out = c.magisk_cli(&["--sqlite", "SELECT uid,policy,until,logging,notification FROM policies"])?;
    let rows = parse_rows(&out);
    if rows.is_empty() {
        println!("  no root policies yet (an app must request su first, or use `wsu su grant <package>`)");
        return Ok(());
    }
    let map = package_map(c).unwrap_or_default();
    for r in &rows {
        let pkg = map
            .iter()
            .find(|(uid, _)| *uid == r.uid || (uid - 100_000) == r.uid)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| "unknown-package".into());
        let pol = match r.policy {
            POLICY_ALLOW => "allow".bright_green(),
            POLICY_DENY => "deny".bright_red(),
            _ => format!("unknown({})", r.policy).bright_yellow(),
        };
        println!("  {}  {}  uid={} log={} notify={}", pkg.bright_cyan(), pol, r.uid, r.logging, r.notification);
    }
    Ok(())
}

fn resolve_uid(c: &Container, pkg: &str) -> Result<i64> {
    let map = package_map(c)?;
    let (found_uid, _) = map
        .into_iter()
        .find(|(_, p)| *p == pkg)
        .with_context(|| format!("package '{pkg}' not installed in Waydroid (check `wsu app list`)"))?;
    Ok(found_uid)
}

pub fn grant(c: &Container, pkg: &str, deny: bool) -> Result<()> {
    c.require()?;
    require_root()?;
    let uid = resolve_uid(c, pkg)?;
    let policy = if deny { POLICY_DENY } else { POLICY_ALLOW };
    let sql = format!(
        "INSERT OR REPLACE INTO policies (uid,policy,until,logging,notification) VALUES ({uid},{policy},0,1,1)"
    );
    c.magisk_cli(&["--sqlite", &sql])?;
    if deny {
        msg_ok(&format!("{pkg} (uid {uid}) is now DENIED root"));
    } else {
        msg_ok(&format!("{pkg} (uid {uid}) is now GRANTED root"));
    }
    Ok(())
}

pub fn revoke(c: &Container, pkg: &str) -> Result<()> {
    c.require()?;
    require_root()?;
    let uid = resolve_uid(c, pkg)?;
    c.magisk_cli(&["--sqlite", &format!("DELETE FROM policies WHERE uid={uid}")])?;
    msg_ok(&format!("{pkg} (uid {uid}) root policy revoked"));
    Ok(())
}

/// Run a command inside the container through the su applet (uid 0 via daemon).
pub fn run_su(c: &Container, cmd: &[String]) -> Result<()> {
    c.require()?;
    let joined = cmd.join(" ");
    let out = c.shell(&["su", "-c", &joined])?;
    println!("{out}");
    Ok(())
}

// ---- App management (pm wrappers) ----

pub fn app_list(c: &Container, third_party: bool) -> Result<()> {
    let flag: &[&str] = if third_party { &["pm", "list", "packages", "-3"] } else { &["pm", "list", "packages"] };
    let out = c.shell(flag)?;
    for line in out.lines() {
        if let Some(p) = line.trim().strip_prefix("package:") {
            println!("  {p}");
        }
    }
    Ok(())
}

/// Install one or more APKs from the HOST into Waydroid. Fails only at the
/// end if any APK could not be installed, so globs keep working.
pub fn app_install(c: &Container, apks: &[std::path::PathBuf]) -> Result<()> {
    c.require()?;
    require_root()?;
    if !c.container_running() {
        c.start_container()?;
    }
    let mut failed: Vec<String> = Vec::new();
    for apk in apks {
        if !apk.exists() {
            msg_warn(&format!("{} skipped (file not found)", apk.display()));
            failed.push(apk.display().to_string());
            continue;
        }
        msg_main(&format!("installing {}", apk.display()));
        let dest = format!("{CONTAINER_TMP}/wsu-app.apk");
        let res = push_file_to_container(apk, &dest).and_then(|()| {
            let (ok, out) = c.shell_try(&["pm", "install", "-r", &dest]);
            if !out.trim().is_empty() {
                println!("{out}");
            }
            if ok { Ok(()) } else { bail!("pm install failed") }
        });
        let _ = c.shell_try(&["rm", "-f", &dest]);
        match res {
            Ok(()) => msg_ok(&format!("{} installed", apk.display())),
            Err(e) => {
                msg_warn(&format!("{} failed: {e:#}", apk.display()));
                failed.push(apk.display().to_string());
            }
        }
    }
    if !failed.is_empty() {
        bail!("{} of {} APK(s) failed: {}", failed.len(), apks.len(), failed.join(", "));
    }
    Ok(())
}

pub fn app_uninstall(c: &Container, pkg: &str) -> Result<()> {
    c.require()?;
    let (ok, out) = c.shell_try(&["pm", "uninstall", pkg]);
    println!("{out}");
    if !ok {
        bail!("pm uninstall failed for {pkg}");
    }
    msg_ok(&format!("{pkg} uninstalled"));
    Ok(())
}

// ---- Manager app ----

pub fn manager_install(c: &Container) -> Result<()> {
    c.require()?;
    require_root()?;
    if !c.container_running() {
        c.start_container()?;
    }
    let cached = std::path::PathBuf::from(WSU_STATE_DIR).join(APK_ASSET);
    if cached.exists() {
        install_manager_apk(c, &cached)
    } else {
        let rel = crate::magisk::latest_release()?;
        let dest = std::path::PathBuf::from(WSU_STATE_DIR).join(&rel.apk_name);
        std::fs::create_dir_all(WSU_STATE_DIR)?;
        download(&rel.apk_url, &dest)?;
        chown_root(&dest);
        install_manager_apk(c, &dest)
    }
}

pub fn manager_launch(c: &Container) -> Result<()> {
    c.require()?;
    if !c.session_running() {
        bail!("no Waydroid session running — start it with `waydroid show-full-ui` first");
    }
    run_out("waydroid", &["app", "launch", MANAGER_PKG])?;
    msg_ok(&format!("launched {MANAGER_PKG}"));
    Ok(())
}

#[cfg(test)]
mod su_tests {
    use super::parse_rows;

    /// magisk --sqlite emits rows as `col=val|col2=val2` lines (fork's db.rs
    /// db_exec_for_cli format).
    #[test]
    fn test_parse_rows() {
        let out = "uid=10100|policy=2|until=0|logging=1|notification=1\nuid=10101|policy=1|until=0|logging=1|notification=0\n";
        let rows = parse_rows(out);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].uid, 10100);
        assert_eq!(rows[0].policy, 2);
        assert_eq!(rows[1].policy, 1);
        assert_eq!(rows[1].notification, 0);
    }
}
