use anyhow::{bail, Result};
use colored::Colorize;

use crate::container::Container;
use crate::utils::*;

// Kitsune Hide / SuList / Zygisk control.
//
// Underlying daemon interfaces (verified in the fork source):
//   magisk --hide status|enable|disable|ls
//   magisk --hide add|rm PKG [PROCESS]
//   magisk --hide mode hide|sulist     (next boot)
//   magisk --hide provider core|external (next boot)
//   magisk --denylist status|enable|disable|add PKG [PROC]|rm PKG [PROC]|ls|exec
//   magisk --zygisk-status             (JSON contract)
//   magisk --sqlite "replace into settings (key,value) values('zygisk',1|0)"
//   magisk --sqlite "replace into settings (key,value) values('sulist',1|0)"
fn require_daemon(c: &Container) -> Result<()> {
    let (daemon_ok, _) = c.magisk_cli_try(&["-c"]);
    if !daemon_ok {
        bail!("magisk daemon not reachable — boot Waydroid (sudo wsu restart) and run `sudo wsu setup`");
    }
    Ok(())
}

pub fn hide_status(c: &Container) -> Result<()> {
    require_daemon(c)?;
    let (hs_ok, hs_out) = c.magisk_cli_try(&["--hide", "status"]);
    println!("  magiskhide : {}", if hs_ok { hs_out.replace('\n', " ") } else { "unknown".into() });
    let (dl_ok, dl_out) = c.magisk_cli_try(&["--denylist", "status"]);
    println!("  denylist   : {}", if dl_ok { dl_out.replace('\n', " ") } else { "unknown".into() });
    match settings(c) {
        Ok(s) => {
            println!("  sulist     : {}", if s.sulist { "enabled (next boot)".bright_yellow() } else { "disabled".normal() });
            println!("  zygisk     : {}", if s.zygisk { "enabled".bright_green() } else { "disabled".normal() });
        }
        Err(_) => println!("  settings   : unavailable"),
    }
    if let Ok(z) = c.magisk_cli(&["--zygisk-status"]) {
        println!("  zygisk contract: {}", z.replace('\n', " "));
    }
    Ok(())
}

pub struct Settings {
    pub zygisk: bool,
    pub sulist: bool,
}

pub fn settings(c: &Container) -> Result<Settings> {
    let out = c.magisk_cli(&["--sqlite", "SELECT key,value FROM settings WHERE key IN ('zygisk','sulist')"])?;
    let mut s = Settings { zygisk: false, sulist: false };
    for line in out.lines() {
        for kv in line.split('|') {
            if let Some((k, v)) = kv.split_once('=') {
                match k {
                    "zygisk" => s.zygisk = v == "1",
                    "sulist" => s.sulist = v == "1",
                    _ => {}
                }
            }
        }
    }
    Ok(s)
}

pub fn set_setting(c: &Container, key: &str, value: bool) -> Result<()> {
    require_daemon(c)?;
    let v = if value { 1 } else { 0 };
    c.magisk_cli(&["--sqlite", &format!("REPLACE INTO settings (key,value) VALUES ('{key}',{v})")])?;
    Ok(())
}

pub fn hide_proxy(c: &Container, args: &[String]) -> Result<()> {
    require_daemon(c)?;
    let owned: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let mut full = vec!["--hide"];
    full.extend_from_slice(&owned);
    let (ok, out) = c.magisk_cli_try(&full);
    if !out.trim().is_empty() {
        println!("{out}");
    }
    if !ok {
        bail!("`magisk --hide {}` failed", owned.join(" "));
    }
    Ok(())
}

pub fn denylist_proxy(c: &Container, args: &[String]) -> Result<()> {
    require_daemon(c)?;
    let owned: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let mut full = vec!["--denylist"];
    full.extend_from_slice(&owned);
    let (ok, out) = c.magisk_cli_try(&full);
    if !out.trim().is_empty() {
        println!("{out}");
    }
    if !ok {
        bail!("`magisk --denylist {}` failed", owned.join(" "));
    }
    Ok(())
}

pub fn zygisk(c: &Container, enable: bool) -> Result<()> {
    require_daemon(c)?;
    set_setting(c, "zygisk", enable)?;
    msg_ok(&format!(
        "zygisk {} — restart Waydroid to apply: sudo wsu restart",
        if enable { "ENABLED" } else { "DISABLED" }
    ));
    Ok(())
}

pub fn sulist(c: &Container, enable: bool) -> Result<()> {
    require_daemon(c)?;
    let mode = if enable { "sulist" } else { "hide" };
    let (ok, out) = c.magisk_cli_try(&["--hide", "mode", mode]);
    if !ok {
        bail!("failed to set hide mode: {out}");
    }
    msg_ok(&format!("hide mode set to {mode} — takes effect on next boot (sudo wsu restart)"));
    Ok(())
}

pub fn sulist_status(c: &Container) -> Result<()> {
    require_daemon(c)?;
    match settings(c) {
        Ok(s) => {
            println!(
                "  sulist: {}",
                if s.sulist { "enabled".bright_green() } else { "disabled (mode: hide)".normal() }
            );
            Ok(())
        }
        Err(_) => bail!("cannot read sulist setting"),
    }
}
