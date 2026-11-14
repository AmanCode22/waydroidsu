use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::constants::*;
use crate::container::Container;
use crate::utils::*;

#[derive(Debug, Clone)]
pub struct Module {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub disabled: bool,
    pub marked_remove: bool,
}

/// List installed modules by walking /data/adb/modules inside the container.
pub fn list(c: &Container) -> Result<Vec<Module>> {
    let script = r#"
for d in /data/adb/modules/*; do
  [ -d "$d" ] || continue
  id=$(grep -m1 '^id=' "$d/module.prop" 2>/dev/null | cut -d= -f2-)
  name=$(grep -m1 '^name=' "$d/module.prop" 2>/dev/null | cut -d= -f2-)
  ver=$(grep -m1 '^version=' "$d/module.prop" 2>/dev/null | cut -d= -f2-)
  author=$(grep -m1 '^author=' "$d/module.prop" 2>/dev/null | cut -d= -f2-)
  echo "WSU_MOD|id=${id:-$(basename $d)}|name=${name:-?}|version=${ver:-?}|author=${author:-?}|disabled=$([ -f $d/disable ] && echo 1 || echo 0)|remove=$([ -f $d/remove ] && echo 1 || echo 0)"
done
"#;
    let out = c.shell(&["sh", "-c", script])?;
    let mut mods = Vec::new();
    for line in out.lines() {
        if !line.starts_with("WSU_MOD|") {
            continue;
        }
        let mut m = Module {
            id: String::new(),
            name: String::new(),
            version: String::new(),
            author: String::new(),
            disabled: false,
            marked_remove: false,
        };
        for kv in line.split('|').skip(1) {
            if let Some((k, v)) = kv.split_once('=') {
                match k {
                    "id" => m.id = v.to_string(),
                    "name" => m.name = v.to_string(),
                    "version" => m.version = v.to_string(),
                    "author" => m.author = v.to_string(),
                    "disabled" => m.disabled = v == "1",
                    "remove" => m.marked_remove = v == "1",
                    _ => {}
                }
            }
        }
        mods.push(m);
    }
    Ok(mods)
}

pub fn print_list(c: &Container) -> Result<()> {
    let mods = list(c)?;
    if mods.is_empty() {
        println!("  no modules installed (install one: wsu module install <zip|url|query>)");
        return Ok(());
    }
    for m in &mods {
        let flags = {
            let mut f = Vec::new();
            if m.disabled {
                f.push("disabled");
            }
            if m.marked_remove {
                f.push("remove-on-reboot");
            }
            if f.is_empty() {
                String::new()
            } else {
                format!("  [{}]", f.join(", "))
            }
        };
        let author = if m.author.is_empty() { "unknown".to_string() } else { m.author.clone() };
        println!(
            "  {} {} {}  by {}{}",
            m.id.bright_cyan(),
            "-".bright_black(),
            m.name,
            author,
            flags.bright_yellow()
        );
        println!("       version: {}", m.version);
    }
    Ok(())
}

/// Resolve any supported source into a local zip path on the host.
/// Sources: local zip path | https URL | repo query / owner-repo name.
pub fn resolve_zip(source: &str) -> Result<PathBuf> {
    let p = PathBuf::from(source);
    if p.exists() && (source.ends_with(".zip") || p.is_file()) {
        return Ok(p);
    }
    if source.starts_with("http://") || source.starts_with("https://") {
        let dest = std::env::temp_dir().join("wsu-module.zip");
        download(source, &dest)?;
        return Ok(dest);
    }
    // repo lookup
    let (full_name, zip_url) = repo_resolve(source)?;
    msg_main(&format!("repo match: {full_name}"));
    let dest = std::env::temp_dir().join("wsu-module.zip");
    download(&zip_url, &dest)?;
    Ok(dest)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoHit {
    pub full_name: String,
    pub description: String,
    pub stars: i64,
    pub url: String,
}

pub fn repo_search(query: &str) -> Result<Vec<RepoHit>> {
    let q = format!("{} magisk module", query.replace(' ', "+"));
    let v = fetch_json(&format!(
        "{GITHUB_API}/search/repositories?q={q}&per_page=8&sort=best-match"
    ))?;
    let mut hits = Vec::new();
    if let Some(items) = v["items"].as_array() {
        for it in items {
            hits.push(RepoHit {
                full_name: it["full_name"].as_str().unwrap_or("").to_string(),
                description: it["description"].as_str().unwrap_or("").to_string(),
                stars: it["stargazers_count"].as_i64().unwrap_or(0),
                url: it["html_url"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    Ok(hits)
}

pub fn print_search(query: &str) -> Result<()> {
    let hits = repo_search(query)?;
    if hits.is_empty() {
        println!("  no repositories matched '{query}'");
        return Ok(());
    }
    for (i, h) in hits.iter().enumerate() {
        println!(
            "  {}. {}  (★{} {})",
            i + 1,
            h.full_name.bright_cyan(),
            h.stars,
            h.url
        );
        if !h.description.is_empty() {
            println!("     {}", h.description);
        }
    }
    println!();
    println!("  install one:  wsu module install <name or owner/repo or zip url>");
    Ok(())
}

/// Resolve a repo query to (full_name, zip_url).
fn repo_resolve(query: &str) -> Result<(String, String)> {
    // Direct owner/repo form: try latest release zip, then module.zip fallback.
    if query.matches('/').count() == 1 && !query.contains(' ') {
        if let Some(url) = repo_zip_url(query)? {
            return Ok((query.to_string(), url));
        }
    }
    let hits = repo_search(query)?;
    let hit = hits
        .first()
        .cloned()
        .with_context(|| format!("no repositories matched '{query}'"))?;
    let url = repo_zip_url(&hit.full_name)?
        .with_context(|| format!("no module zip found for {}", hit.full_name))?;
    Ok((hit.full_name, url))
}

/// Find a downloadable zip for a GitHub repo: latest release .zip asset,
/// falling back to module.zip on the default branch.
fn repo_zip_url(full_name: &str) -> Result<Option<String>> {
    if let Ok(rel) = fetch_json(&format!("{GITHUB_API}/repos/{full_name}/releases/latest")) {
        if let Some(assets) = rel["assets"].as_array() {
            for a in assets {
                let name = a["name"].as_str().unwrap_or("");
                if name.ends_with(".zip") {
                    return Ok(a["browser_download_url"].as_str().map(String::from));
                }
            }
        }
    }
    // module.zip convention on the default branch
    let branch = match fetch_json(&format!("{GITHUB_API}/repos/{full_name}")) {
        Ok(v) => v["default_branch"].as_str().unwrap_or("main").to_string(),
        Err(_) => "main".into(),
    };
    Ok(Some(format!("{RAW_GH}/{full_name}/{branch}/module.zip")))
}

/// Install a module from any supported source into the running container.
pub fn install(c: &Container, source: &str) -> Result<()> {
    c.require()?;
    require_root()?;
    if !c.container_running() {
        c.start_container()?;
    }
    let (daemon_ok, _) = c.magisk_cli_try(&["-c"]);
    if !daemon_ok {
        bail!("magisk daemon is not reachable inside the container (boot Waydroid and run `sudo wsu setup` first)");
    }
    let zip = resolve_zip(source)?;
    msg_main(&format!("installing module from {}", zip.display()));
    let dest = format!("{CONTAINER_TMP}/wsu-module.zip");
    push_file_to_container(&zip, &dest)?;
    let (ok, out) = c.magisk_cli_try(&["--install-module", &dest]);
    println!("{}", out);
    let _ = c.shell_try(&["rm", "-f", &dest]);
    if !ok {
        bail!("module installation failed");
    }
    msg_ok("module installed — it activates on next Waydroid boot (sudo wsu restart)");
    Ok(())
}

pub fn remove(c: &Container, id: &str, auto_yes: bool) -> Result<()> {
    c.require()?;
    let dir = format!("{CONTAINER_MODULES_DIR}/{id}");
    let (dir_ok, _) = c.shell_try(&["test", "-d", &dir]);
    if !dir_ok {
        bail!("module '{id}' not found (see `wsu module list`)");
    }
    if !ask_yes(&format!("mark module '{id}' for removal (applied on next boot)?"), auto_yes) {
        bail!("aborted");
    }
    c.shell(&["touch", &format!("{dir}/remove")])?;
    msg_ok(&format!("module '{id}' marked for removal — takes effect on next boot"));
    Ok(())
}

pub fn set_enabled(c: &Container, id: &str, enable: bool) -> Result<()> {
    c.require()?;
    let dir = format!("{CONTAINER_MODULES_DIR}/{id}");
    let (dir_ok, _) = c.shell_try(&["test", "-d", &dir]);
    if !dir_ok {
        bail!("module '{id}' not found (see `wsu module list`)");
    }
    if enable {
        c.shell(&["rm", "-f", &format!("{dir}/disable")])?;
        msg_ok(&format!("module '{id}' enabled — takes effect on next boot"));
    } else {
        c.shell(&["touch", &format!("{dir}/disable")])?;
        msg_ok(&format!("module '{id}' disabled — takes effect on next boot"));
    }
    Ok(())
}
