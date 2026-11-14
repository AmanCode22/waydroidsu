use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::constants::*;
use crate::container::Container;
use crate::utils::*;

/// Latest Kitsune release metadata from the maintained fork.
pub struct ReleaseInfo {
    pub tag: String,
    pub apk_url: String,
    pub apk_name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WsuState {
    pub tag: String,
    pub base: String,
    pub abi: String,
    pub installed_at: String,
}

pub fn latest_release() -> Result<ReleaseInfo> {
    let url = format!("{GITHUB_API}/repos/{KITSUNE_REPO}/releases/latest");
    let v = match fetch_json(&url) {
        Ok(v) => v,
        Err(_) => {
            let list = fetch_json(&format!("{GITHUB_API}/repos/{KITSUNE_REPO}/releases?per_page=10"))?;
            list.as_array()
                .and_then(|a| a.first())
                .cloned()
                .context("no releases found in the Kitsune fork")?
        }
    };
    let tag = v["tag_name"]
        .as_str()
        .context("release missing tag_name")?
        .to_string();
    let assets = v["assets"].as_array().context("release missing assets")?;
    let asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(APK_ASSET))
        .or_else(|| assets.iter().find(|a| a["name"].as_str().unwrap_or("").ends_with(".apk")))
        .context("release has no .apk asset")?;
    Ok(ReleaseInfo {
        tag,
        apk_url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
        apk_name: asset["name"].as_str().unwrap_or(APK_ASSET).to_string(),
    })
}

pub fn load_state() -> Option<WsuState> {
    let p = PathBuf::from(WSU_STATE_DIR).join("wsu.json");
    let text = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_state(state: &WsuState) -> Result<()> {
    let dir = PathBuf::from(WSU_STATE_DIR);
    std::fs::create_dir_all(&dir)?;
    let p = dir.join("wsu.json");
    std::fs::write(&p, serde_json::to_string_pretty(state)?)?;
    chown_root(&p);
    Ok(())
}

fn abi32_of(abi: &str) -> Option<&'static str> {
    match abi {
        "x86_64" => Some("x86"),
        "arm64-v8a" => Some("armeabi-v7a"),
        _ => None,
    }
}

/// Copy an APK from host into the container and install it as the manager app.
pub fn install_manager_apk(c: &Container, apk: &Path) -> Result<()> {
    msg_main("installing Kitsune manager app inside Waydroid");
    let tmp_apk = format!("{CONTAINER_TMP}/wsu-manager.apk");
    push_file_to_container(apk, &tmp_apk)?;
    let (ok, err) = c.shell_try(&["pm", "install", "-r", "-g", &tmp_apk]);
    if !ok {
        bail!("pm install failed: {err}");
    }
    msg_ok("manager app installed");
    // Pre-grant the manager root so first-run friction is zero.
    let (_, out) = c.shell_try(&["pm", "list", "packages", "-U"]);
    if let Some(uid) = uid_of_package(&out, MANAGER_PKG) {
        let sql = format!(
            "INSERT OR REPLACE INTO policies (uid,policy,until,logging,notification) VALUES ({uid},{POLICY_ALLOW},0,1,1)"
        );
        match c.magisk_cli(&["--sqlite", &sql]) {
            Ok(_) => msg_ok(&format!("root pre-granted to {MANAGER_PKG} (uid {uid})")),
            Err(_) => msg_warn("could not pre-grant root (daemon may need a reboot)"),
        }
    }
    let _ = c.shell_try(&["rm", "-f", &tmp_apk]);
    Ok(())
}

pub fn uid_of_package(pm_out: &str, pkg: &str) -> Option<i64> {
    for line in pm_out.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&format!("package:{pkg} ")) {
            if rest.trim() == pkg {
                // same-name package with no uid suffix
                continue;
            }
            for tok in rest.split_whitespace() {
                if let Some(u) = tok.strip_prefix("uid:") {
                    return u.parse::<i64>().ok();
                }
            }
        }
    }
    None
}

/// Append (or refresh) the wsu rc block on bootanim.rc with a backup.
fn patch_bootanim_rc(sys: &Path) -> Result<()> {
    let init_dir = sys.join("etc/init");
    std::fs::create_dir_all(&init_dir)?;
    let rc_plain = init_dir.join(BOOTANIM_RC_REL);
    let rc_gz = init_dir.join(BOOTANIM_RC_GZ_REL);

    if rc_plain.exists() {
        let backup = init_dir.join(format!("{BOOTANIM_RC_REL}{RC_BACKUP_SUFFIX}"));
        if !backup.exists() {
            std::fs::copy(&rc_plain, &backup)?;
            chown_root(&backup);
        }
        let mut content = std::fs::read_to_string(&rc_plain).unwrap_or_default();
        if content.contains(RC_BLOCK_BEGIN) {
            // strip old block so we refresh it
            if let (Some(b), Some(e)) = (content.find(RC_BLOCK_BEGIN), content.find(RC_BLOCK_END)) {
                let end = e + RC_BLOCK_END.len();
                content.replace_range(b..end, "");
            }
        }
        content.push('\n');
        content.push_str(RC_BLOCK);
        std::fs::write(&rc_plain, content)?;
        set_mode(&rc_plain, 0o644)?;
        chown_root(&rc_plain);
        return Ok(());
    }

    if rc_gz.exists() {
        let backup = init_dir.join(format!("{BOOTANIM_RC_GZ_REL}{RC_BACKUP_SUFFIX}"));
        if !backup.exists() {
            std::fs::copy(&rc_gz, &backup)?;
            chown_root(&backup);
        }
        // Decompress, append, write as plain rc and move the gz aside.
        let (_, out) = run_try(
            "sh",
            &["-c", &format!("gunzip -c '{}' ", rc_gz.display())],
        );
        if out.is_empty() {
            bail!("could not decompress {}", rc_gz.display());
        }
        std::fs::write(&rc_plain, format!("{out}\n{RC_BLOCK}"))?;
        set_mode(&rc_plain, 0o644)?;
        chown_root(&rc_plain);
        let side = init_dir.join(format!("{BOOTANIM_RC_GZ_REL}{RC_BACKUP_SUFFIX}"));
        std::fs::rename(&rc_gz, &side)?;
        return Ok(());
    }

    // No bootanim rc in the image: ship the full fallback (AOSP bootanim + block).
    std::fs::write(&rc_plain, BOOTANIM_RC_FALLBACK)?;
    set_mode(&rc_plain, 0o644)?;
    chown_root(&rc_plain);
    Ok(())
}

fn restore_bootanim_rc(sys: &Path) {
    let init_dir = sys.join("etc/init");
    let rc_plain = init_dir.join(BOOTANIM_RC_REL);
    let rc_gz = init_dir.join(BOOTANIM_RC_GZ_REL);
    // Case: we created a fresh rc (no original existed).
    let plain_bak = init_dir.join(format!("{BOOTANIM_RC_REL}{RC_BACKUP_SUFFIX}"));
    let gz_bak = init_dir.join(format!("{BOOTANIM_RC_GZ_REL}{RC_BACKUP_SUFFIX}"));
    if !plain_bak.exists() && rc_plain.exists() {
        let _ = std::fs::remove_file(&rc_plain);
    }
    if plain_bak.exists() {
        let _ = std::fs::copy(&plain_bak, &rc_plain);
        let _ = std::fs::remove_file(&plain_bak);
    }
    if gz_bak.exists() {
        let _ = std::fs::copy(&gz_bak, &rc_gz);
        let _ = std::fs::remove_file(&gz_bak);
    }
}

fn write_payload(c: &Container, apk: &Path, abi: &str) -> Result<(PathBuf, String)> {
    let (sys, base) = c.system_root()?;
    let magisk_dir = sys.join(MAGISK_DIR_REL);
    std::fs::create_dir_all(&magisk_dir)?;

    msg_main(&format!("extracting Kitsune payload ({abi}) into {}", magisk_dir.display()));
    let mut wants: Vec<(&str, &str)> = PAYLOAD_LIBS
        .iter()
        .filter(|(lib, _)| *lib != "libmagisk32.so")
        .cloned()
        .collect();
    let extracted = extract_zip_members(apk, &magisk_dir, &format!("lib/{abi}"), &wants)?;
    if extracted.is_empty() {
        bail!("APK does not contain lib/{abi} payloads — wrong architecture? Use --abi to override");
    }

    // 32-bit daemon binary (needed only when the image supports 32-bit ABIs).
    let rootfs_bin = PathBuf::from(ROOTFS_SYSTEM).join("bin/linker");
    if let Some(abi32) = abi32_of(abi) {
        if rootfs_bin.exists() {
            let e = extract_zip_members(apk, &magisk_dir, &format!("lib/{abi32}"), &[("libmagisk.so", "magisk32")])?;
            if !e.is_empty() {
                msg_ok("32-bit companion binary (magisk32) extracted");
            }
        }
    }
    wants.clear();

    let assets = extract_zip_assets(apk, &magisk_dir, "assets/")?;
    msg_ok(&format!("extracted {} binaries + {} script assets", extracted.len(), assets.len()));

    // marker: which release this payload came from
    let tag = load_state().map(|s| s.tag).unwrap_or_default();
    let _ = std::fs::write(magisk_dir.join("wsu-release"), &tag);

    for f in std::fs::read_dir(&magisk_dir)? {
        let f = f?.path();
        chown_root(&f);
    }
    Ok((magisk_dir, base.to_string()))
}

pub fn install(c: &Container, apk: Option<PathBuf>, abi: Option<String>, auto_yes: bool) -> Result<()> {
    c.require()?;
    require_root()?;

    let abi = abi.unwrap_or_else(detect_abi);
    let running = c.container_running() || c.session_running();
    if running && ask_yes("Waydroid is running; stop it now to install safely?", auto_yes) {
        c.stop_all()?;
    }

    // 1) obtain the manager/release APK
    let apk_path = match apk {
        Some(p) => {
            if !p.exists() {
                bail!("--apk file not found: {}", p.display());
            }
            msg_main(&format!("using local APK {}", p.display()));
            p
        }
        None => {
            let rel = latest_release()?;
            msg_main(&format!("latest Kitsune release: {}", rel.tag));
            let dest = PathBuf::from(WSU_STATE_DIR).join(&rel.apk_name);
            std::fs::create_dir_all(WSU_STATE_DIR)?;
            if dest.exists() && std::fs::metadata(&dest).is_ok() {
                if ask_yes(&format!("cached {} found, reuse? (no = re-download)", rel.apk_name), auto_yes) {
                    msg_sub("using cached APK");
                } else {
                    download(&rel.apk_url, &dest)?;
                }
            } else {
                download(&rel.apk_url, &dest)?;
            }
            chown_root(&dest);
            dest
        }
    };

    // 2) payload into the waydroid system tree
    let (magisk_dir, base) = write_payload(c, &apk_path, &abi)?;

    // 3) boot bootstrap script + rc injection
    let (sys, _) = c.system_root()?;
    let boot_sh = magisk_dir.join("wsu_boot.sh");
    std::fs::write(&boot_sh, WSU_BOOT_SH)?;
    set_mode(&boot_sh, 0o755)?;
    chown_root(&boot_sh);
    patch_bootanim_rc(&sys)?;
    msg_ok("bootanim.rc patched with the wsu bootstrap block");

    // 4) state
    let state = WsuState {
        tag: latest_release().map(|r| r.tag).unwrap_or_else(|_| "local-apk".into()),
        base,
        abi,
        installed_at: host_date(),
    };
    save_state(&state)?;

    msg_main("install complete");
    println!("  next: start Waydroid (waydroid show-full-ui) and run:  sudo wsu setup");
    Ok(())
}

pub fn setup(c: &Container, _auto_yes: bool) -> Result<()> {
    c.require()?;
    if !c.is_installed() {
        bail!("Magisk is not installed yet — run `sudo wsu install` first");
    }
    if !c.container_running() {
        c.start_container()?;
    }
    msg_main("waiting for Android to finish booting");
    let mut booted = false;
    for _ in 0..90 {
        let (_, out) = c.shell_try(&["getprop", "sys.boot_completed"]);
        if out.trim() == "1" {
            booted = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    if !booted {
        bail!("container did not report sys.boot_completed=1 within 180s");
    }
    msg_ok("boot completed");

    msg_main("verifying Kitsune daemon");
    let (sbin_ok, _) = c.shell_try(&["test", "-x", CONTAINER_MAGISK_BIN]);
    if !sbin_ok {
        bail!("{CONTAINER_MAGISK_BIN} not present — the rc bootstrap did not run; check `sudo wsu status`");
    }
    let (ok, ver) = c.magisk_cli_try(&["-c"]);
    if ok {
        msg_ok(&format!("daemon running: {ver}"));
    } else {
        msg_warn(&format!("daemon not reachable yet ({ver}) — a container restart may be needed"));
    }
    print_features(c);

    // manager app
    let (_, installed) = c.shell_try(&["pm", "path", MANAGER_PKG]);
    let manager_present = !installed.trim().is_empty() && installed.contains("package:");
    if manager_present {
        msg_ok("manager app already installed");
    } else {
        let cached = PathBuf::from(WSU_STATE_DIR).join(APK_ASSET);
        if cached.exists() {
            install_manager_apk(c, &cached)?;
        } else {
            msg_warn("no cached manager APK found — run `sudo wsu manager install`");
        }
    }
    zygisk_report(c);
    Ok(())
}

fn print_features(c: &Container) {
    if let Ok(feat) = c.magisk_cli(&["--features"]) {
        msg_ok(&format!("capabilities: {}", feat.replace('\n', " ")));
    }
}

fn zygisk_report(c: &Container) {
    match c.magisk_cli(&["--zygisk-status"]) {
        Ok(s) => msg_ok(&format!("zygisk: {}", s.replace('\n', " "))),
        Err(_) => msg_warn("zygisk status unavailable (daemon may be down)"),
    }
}

pub fn remove(c: &Container, auto_yes: bool) -> Result<()> {
    c.require()?;
    require_root()?;
    if !c.is_installed() {
        msg_warn("wsu/magisk is not installed");
        return Ok(());
    }
    if !ask_yes("Remove KitsuneMagisk from Waydroid?", auto_yes) {
        bail!("aborted");
    }
    if c.container_running() {
        msg_sub("stopping magisk daemon inside container");
        let _ = c.magisk_cli_try(&["--stop"]);
        c.stop_all()?;
    }
    let (sys, _) = c.system_root()?;
    let magisk_dir = sys.join(MAGISK_DIR_REL);
    if magisk_dir.exists() {
        std::fs::remove_dir_all(&magisk_dir)?;
        msg_ok("removed payload directory");
    }
    restore_bootanim_rc(&sys);
    msg_ok("restored bootanim.rc");

    // container-side cleanup
    let _ = c.shell_try(&["rm", "-rf", CONTAINER_MAGISKBIN, CONTAINER_MODULES_DIR, "/data/adb/post-fs-data.d", "/data/adb/service.d"]);
    let _ = std::fs::remove_file(PathBuf::from(WSU_STATE_DIR).join("wsu.json"));
    msg_main("KitsuneMagisk removed");
    Ok(())
}

pub fn update(c: &Container, check_only: bool, auto_yes: bool) -> Result<()> {
    let rel = latest_release()?;
    let installed = load_state().map(|s| s.tag).unwrap_or_else(|| {
        let d = c.magisk_dir_host().ok().map(|d| d.join("wsu-release"));
        d.and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default()
    });
    if installed == rel.tag {
        msg_ok(&format!("already on the latest release {rel}"));
        return Ok(());
    }
    if check_only {
        msg_main(&format!("update available: {installed} -> {rel}"));
        println!("  run: sudo wsu update");
        return Ok(());
    }
    c.require()?;
    msg_main(&format!("updating {installed} -> {rel}"));
    // clear cached apk so we fetch the new one
    let cache = PathBuf::from(WSU_STATE_DIR).join(APK_ASSET);
    let _ = std::fs::remove_file(&cache);
    install(c, None, None, auto_yes)
}

pub fn restart(c: &Container) -> Result<()> {
    c.require()?;
    msg_main("restarting waydroid");
    c.stop_all()?;
    c.start_container()?;
    Ok(())
}

pub fn host_date() -> String {
    run_out("sh", &["-c", "date -u +%Y-%m-%dT%H:%M:%SZ"]).unwrap_or_else(|_| "unknown".into())
}

impl std::fmt::Display for ReleaseInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.tag)
    }
}
