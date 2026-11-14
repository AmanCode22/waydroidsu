use anyhow::{anyhow, bail, Context, Result};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn msg_main(msg: &str) {
    println!("{}", format!("=> {msg}").bright_cyan());
}

pub fn msg_sub(msg: &str) {
    println!("  {} {msg}", "-".bright_black());
}

pub fn msg_ok(msg: &str) {
    println!("{}", format!("  [OK] {msg}").bright_green());
}

pub fn msg_warn(msg: &str) {
    println!("{}", format!("  [!] {msg}").bright_yellow());
}

pub fn msg_err(msg: &str) {
    eprintln!("{}", format!("  [X] {msg}").bright_red());
}

use colored::Colorize;

/// Run a host command, return trimmed stdout; error includes stderr.
pub fn run_out(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to spawn `{program}`"))?;
    if !out.status.success() {
        bail!(
            "`{program} {}` failed ({}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run a host command, return (success, combined output) without failing.
pub fn run_try(program: &str, args: &[&str]) -> (bool, String) {
    match Command::new(program).args(args).stdin(Stdio::null()).output() {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.success(), text.trim().to_string())
        }
        Err(_) => (false, String::new()),
    }
}

pub fn require_root() -> Result<()> {
    if !is_root() {
        bail!("this command needs root — run it with sudo (e.g. `sudo wsu install`)");
    }
    Ok(())
}

pub fn is_root() -> bool {
    nix_free_uid() == 0
}

// euid via /proc, avoids adding the nix crate.
fn nix_free_uid() -> u32 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
        })
        .unwrap_or(1000)
}

/// Download `url` to `dest` with curl (retries, follows redirects, shows errors).
pub fn download(url: &str, dest: &Path) -> Result<()> {
    msg_sub(&format!("downloading {url}"));
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = Command::new("curl")
        .args([
            "-fSL",
            "--retry",
            "3",
            "--connect-timeout",
            "20",
            "-o",
            dest.to_str().unwrap_or_default(),
            url,
        ])
        .stdin(Stdio::null())
        .output()
        .context("curl not available")?;
    if !out.status.success() {
        bail!("download failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let meta = std::fs::metadata(dest)?;
    if meta.len() == 0 {
        bail!("downloaded file is empty");
    }
    Ok(())
}

/// GET a JSON document (GitHub API). Uses GITHUB_TOKEN when set.
pub fn fetch_json(url: &str) -> Result<serde_json::Value> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "--connect-timeout", "20", "-H", "Accept: application/vnd.github+json"]);
    cmd.arg("-H").arg(format!("User-Agent: wsu/{} (WaydroidSU)", crate::constants::WSU_VERSION));
    if let Ok(tok) = std::env::var("GITHUB_TOKEN") {
        if !tok.trim().is_empty() {
            cmd.arg("-H").arg(format!("Authorization: Bearer {}", tok.trim()));
        }
    }
    cmd.arg(url).stdin(Stdio::null());
    let out = cmd.output().context("curl not available")?;
    if !out.status.success() {
        bail!(
            "GitHub API request failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    serde_json::from_slice(&out.stdout).with_context(|| format!("bad JSON from {url}"))
}

/// uname -m -> Android ABI used inside the APK (lib/<abi>/...).
pub fn detect_abi() -> String {
    let m = run_out("uname", &["-m"]).unwrap_or_default();
    match m.as_str() {
        "x86_64" | "amd64" => "x86_64".into(),
        "aarch64" | "arm64" => "arm64-v8a".into(),
        "i386" | "i486" | "i586" | "i686" => "x86".into(),
        "armv7l" | "armv8l" => "armeabi-v7a".into(),
        other => {
            msg_warn(&format!("unknown uname -m '{other}', defaulting to x86_64"));
            "x86_64".into()
        }
    }
}

/// Extract selected members of a zip archive into `dest_dir`.
/// `wants` maps archive member name -> destination file name.
pub fn extract_zip_members(
    archive: &Path,
    dest_dir: &Path,
    prefix_dir: &str,
    wants: &[(&str, &str)],
) -> Result<Vec<PathBuf>> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    std::fs::create_dir_all(dest_dir)?;
    let mut extracted = Vec::new();
    for (member, dest_name) in wants {
        let full = format!("{prefix_dir}/{member}");
        let mut zf = match zip.by_name(&full) {
            Ok(z) => z,
            Err(_) => continue, // optional payload
        };
        let mut buf = Vec::with_capacity(zf.size() as usize);
        std::io::Read::read_to_end(&mut zf, &mut buf)?;
        let out_path = dest_dir.join(dest_name);
        let mut f = std::fs::File::create(&out_path)?;
        f.write_all(&buf)?;
        set_mode(&out_path, 0o755)?;
        extracted.push(out_path);
    }
    Ok(extracted)
}

/// Extract every member under `prefix_dir` whose name ends with `.sh`/`.apk`
/// (flattened) into `dest_dir`.
pub fn extract_zip_assets(archive: &Path, dest_dir: &Path, prefix: &str) -> Result<Vec<PathBuf>> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    std::fs::create_dir_all(dest_dir)?;
    let mut extracted = Vec::new();
    for i in 0..zip.len() {
        let mut zf = match zip.by_index(i) {
            Ok(z) => z,
            Err(_) => continue,
        };
        let name = match zf.name() {
            n if n.starts_with(prefix) && !n.ends_with('/') => n.to_string(),
            _ => continue,
        };
        let base = name.rsplit('/').next().unwrap_or_default().to_string();
        if !(base.ends_with(".sh") || base.ends_with(".apk")) {
            continue;
        }
        let mut buf = Vec::with_capacity(zf.size() as usize);
        std::io::Read::read_to_end(&mut zf, &mut buf)?;
        let out_path = dest_dir.join(&base);
        let mut f = std::fs::File::create(&out_path)?;
        f.write_all(&buf)?;
        set_mode(&out_path, if base.ends_with(".apk") { 0o644 } else { 0o755 })?;
        extracted.push(out_path);
    }
    Ok(extracted)
}

pub fn set_mode(path: &Path, mode: u32) -> Result<()> {
    run_out("chmod", &[&format!("{mode:04o}"), path.to_str().unwrap_or_default()])?;
    Ok(())
}

/// Best-effort chown to root:root — failures are non-fatal (overlay trees may
/// have odd ownership and non-root callers skip this entirely).
pub fn chown_root(path: &Path) {
    if is_root() {
        let _ = run_out("chown", &["root:root", path.to_str().unwrap_or_default()]);
    }
}

pub fn ask_yes(question: &str, auto_yes: bool) -> bool {
    if auto_yes {
        println!("  {} {question} (auto: yes)", "?".bright_cyan());
        return true;
    }
    print!("  {} {question} [y/N] ", "?".bright_cyan());
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Copy a file from the HOST into the running container.
/// Primary: stream via stdin (`waydroid shell sh -c 'cat > dest'`).
/// Fallback: base64 chunks appended to a .b64 file then decoded.
/// Returns Ok(()) after verifying the size inside the container.
pub fn push_file_to_container(local: &Path, container_dest: &str) -> Result<()> {
    let size = std::fs::metadata(local)?.len();
    // 1) stdin pipe
    let child = Command::new("waydroid")
        .args(["shell", "sh", "-c", &format!("cat > {container_dest}")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    if let Ok(mut child) = child {
        let data = std::fs::read(local)?;
        let mut stdin = child.stdin.take().unwrap();
        let written = std::io::Write::write_all(&mut stdin, &data);
        drop(stdin);
        let _ = child.wait();
        if written.is_ok() {
            if let Ok(vsize) = container_file_size(container_dest) {
                if vsize == size {
                    return Ok(());
                }
            }
        }
    }
    msg_warn("stdin piping into `waydroid shell` failed; falling back to base64 chunks");
    // 2) base64 fallback
    let b64 = run_out(
        "base64",
        &["-w", "0", local.to_str().unwrap_or_default()],
    )?;
    cleanup_container_path(&format!("{container_dest}.b64"));
    for chunk in chunk_str(&b64, 60_000) {
        let (ok, err) = run_try(
            "waydroid",
            &["shell", "sh", "-c", &format!("printf '%s' '{chunk}' >> {container_dest}.b64")],
        );
        if !ok {
            bail!("failed to transfer module into container: {err}");
        }
    }
    let (ok, err) = run_try(
        "waydroid",
        &["shell", "sh", "-c", &format!("base64 -d {container_dest}.b64 > {container_dest} && rm -f {container_dest}.b64")],
    );
    if !ok {
        bail!("base64 decode inside container failed: {err}");
    }
    if let Ok(vsize) = container_file_size(container_dest) {
        if vsize == size {
            return Ok(());
        }
        bail!("size mismatch after transfer: host {size}, container {vsize}");
    }
    Ok(())
}

fn container_file_size(path: &str) -> Result<u64> {
    let out = run_out("waydroid", &["shell", "stat", "-c", "%s", path])?;
    out.trim().parse::<u64>().map_err(|e| anyhow!("stat parse: {e}"))
}

pub fn cleanup_container_path(path: &str) {
    let _ = run_try("waydroid", &["shell", "rm", "-f", path]);
}

fn chunk_str(s: &str, n: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        let mut end = (start + n).min(bytes.len());
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        out.push(&s[start..end]);
        start = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::magisk::uid_of_package;

    /// Validate the extractor against a real kitsune-canary APK when present
    /// (skipped on machines without the fixture).
    #[test]
    fn test_extract_real_apk() {
        let apk = std::path::PathBuf::from("/tmp/apk-inspect/kitsune.apk");
        if !apk.exists() {
            return;
        }
        let dest = std::path::PathBuf::from("/tmp/wsu-test-extract");
        let _ = std::fs::remove_dir_all(&dest);
        let wants = [("libmagisk.so", "magisk"), ("libmagiskpolicy.so", "magiskpolicy"), ("libbusybox.so", "busybox")];
        let out = extract_zip_members(&apk, &dest, "lib/x86_64", &wants).unwrap();
        assert_eq!(out.len(), 3, "expected 3 binaries from lib/x86_64");
        let m = std::fs::metadata(dest.join("magisk")).unwrap();
        assert!(m.len() > 400_000, "magisk binary suspiciously small: {}", m.len());
        let assets = extract_zip_assets(&apk, &dest, "assets/").unwrap();
        assert!(assets.iter().any(|p| p.ends_with("stub.apk")), "stub.apk not extracted");
        assert!(assets.iter().any(|p| p.ends_with("util_functions.sh")));
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn test_uid_of_package() {
        let sample = "package:io.github.jordan231111.magisk uid:10123\npackage:com.android.settings uid:1000\n";
        assert_eq!(uid_of_package(sample, "io.github.jordan231111.magisk"), Some(10123));
        assert_eq!(uid_of_package(sample, "com.android.settings"), Some(1000));
        assert_eq!(uid_of_package(sample, "missing.pkg"), None);
    }

    #[test]
    fn test_chunk_str_ascii_and_utf8() {
        assert_eq!(chunk_str("abcdef", 4), vec!["abcd", "ef"]);
        // backward boundary walk: chunks may be smaller than n for multibyte
        // input ('é' spans bytes 1..2, so the first chunk is just "h")
        assert_eq!(chunk_str("héllo", 2), vec!["h", "é", "ll", "o"]);
    }
}
