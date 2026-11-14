mod cli;
mod container;
mod constants;
mod hide;
mod magisk;
mod modules;
mod superuser;
mod utils;

use anyhow::{bail, Result};
use clap::Parser;
use cli::{AppCmd, Cli, Cmd, ManagerCmd, ModuleCmd, RepoCmd, SulistCmd, SuCmd, ZygiskCmd};
use colored::Colorize;
use container::Container;

fn main() {
    let cli = Cli::parse();
    let c = Container::detect();
    let code = match run(&cli, &c) {
        Ok(()) => 0,
        Err(e) => {
            utils::msg_err(&format!("{e:#}"));
            1
        }
    };
    std::process::exit(code);
}

fn run(cli: &Cli, c: &Container) -> Result<()> {
    let yes = cli.yes;
    match &cli.cmd {
        Cmd::Status => status(c),
        Cmd::Install { apk, abi } => magisk::install(c, apk.clone(), abi.clone(), yes),
        Cmd::Setup => magisk::setup(c, yes),
        Cmd::Remove => magisk::remove(c, yes),
        Cmd::Update { check } => magisk::update(c, *check, yes),
        Cmd::Restart => magisk::restart(c),
        Cmd::Manager { cmd } => match cmd {
            ManagerCmd::Install => superuser::manager_install(c),
            ManagerCmd::Launch => superuser::manager_launch(c),
        },
        Cmd::Module { cmd } => match cmd {
            ModuleCmd::List => modules::print_list(c),
            ModuleCmd::Install { source } => modules::install(c, source),
            ModuleCmd::Remove { id } => modules::remove(c, id, yes),
            ModuleCmd::Enable { id } => modules::set_enabled(c, id, true),
            ModuleCmd::Disable { id } => modules::set_enabled(c, id, false),
        },
        Cmd::Repo { cmd } => match cmd {
            RepoCmd::Search { query } => modules::print_search(query),
        },
        Cmd::MagiskHide { args } => {
            if args.is_empty() {
                hide::hide_status(c)
            } else {
                hide::hide_proxy(c, args)
            }
        }
        Cmd::Denylist { args } => {
            if args.is_empty() {
                bail!("usage: wsu denylist -- <status|enable|disable|add PKG [PROC]|rm PKG [PROC]|ls|exec CMD>");
            }
            hide::denylist_proxy(c, args)
        }
        Cmd::Sulist { cmd } => match cmd {
            SulistCmd::Status => hide::sulist_status(c),
            SulistCmd::Enable => hide::sulist(c, true),
            SulistCmd::Disable => hide::sulist(c, false),
        },
        Cmd::Zygisk { cmd } => match cmd {
            ZygiskCmd::Status => {
                utils::msg_main("zygisk status");
                match c.magisk_cli(&["--zygisk-status"]) {
                    Ok(s) => {
                        println!("  contract: {}", s.replace('\n', " "));
                        Ok(())
                    }
                    Err(e) => bail!("daemon not reachable: {e}"),
                }
            }
            ZygiskCmd::Enable => hide::zygisk(c, true),
            ZygiskCmd::Disable => hide::zygisk(c, false),
        },
        Cmd::Su { cmd, passthrough } => match cmd {
            Some(SuCmd::List) => superuser::list(c),
            Some(SuCmd::Grant { package }) => superuser::grant(c, package, false),
            Some(SuCmd::Deny { package }) => superuser::grant(c, package, true),
            Some(SuCmd::Revoke { package }) => superuser::revoke(c, package),
            None => {
                if passthrough.is_empty() {
                    superuser::list(c)
                } else {
                    superuser::run_su(c, passthrough)
                }
            }
        },
        Cmd::App { cmd } => match cmd {
            AppCmd::List { third_party } => superuser::app_list(c, *third_party),
            AppCmd::Install { apk } => superuser::app_install(c, apk),
            AppCmd::Uninstall { package } => superuser::app_uninstall(c, package),
        },
        Cmd::SelfUninstall { keep_magisk } => self_uninstall(c, *keep_magisk, yes),
    }
}

fn status(c: &Container) -> Result<()> {
    println!("{}", constants::WSU_TAGLINE.bright_cyan());
    println!("  wsu       : v{}", constants::WSU_VERSION);
    println!(
        "  upstream  : {} (kitsune-canary releases)",
        constants::KITSUNE_REPO
    );
    if !c.present {
        utils::msg_warn("Waydroid not found — install waydroid and initialize it first");
        return Ok(());
    }
    let st = c.status_text();
    for line in st.lines() {
        println!("  waydroid  : {line}");
    }
    println!(
        "  overlays  : {}",
        if c.overlays_enabled() { "enabled (overlay/system)" } else { "disabled (rootfs/system)" }
    );

    let installed = c.is_installed();
    println!(
        "  magisk    : {}",
        if installed { "installed".bright_green() } else { "not installed".bright_red() }
    );
    if installed {
        if let Ok(d) = c.magisk_dir_host() {
            let tag = std::fs::read_to_string(d.join("wsu-release")).unwrap_or_default();
            if !tag.trim().is_empty() {
                println!("  release   : {}", tag.trim());
            }
        }
        if let Some(s) = magisk::load_state() {
            println!("  installed : {} ({}, abi {})", s.installed_at, s.base, s.abi);
        }
    }

    if c.container_running() {
        let (ok, ver) = c.magisk_cli_try(&["-c"]);
        if ok {
            println!("  daemon    : {}", ver.bright_green());
            if let Ok(s) = hide::settings(c) {
                println!("  zygisk    : {}", if s.zygisk { "enabled".bright_green() } else { "disabled".normal() });
                println!("  sulist    : {}", if s.sulist { "enabled".bright_yellow() } else { "disabled (mode: hide)".normal() });
            }
            if let Ok(mods) = modules::list(c) {
                let active = mods.iter().filter(|m| !m.disabled && !m.marked_remove).count();
                println!("  modules   : {} installed, {} active", mods.len(), active);
            }
            let (mgr_ok, _) = c.shell_try(&["pm", "path", constants::MANAGER_PKG]);
            println!(
                "  manager   : {}",
                if mgr_ok { constants::MANAGER_PKG.bright_green() } else { "not installed".bright_red() }
            );
        } else {
            println!("  daemon    : {}", "not reachable (run `sudo wsu setup`)".bright_yellow());
        }
    } else {
        println!("  daemon    : container not running");
    }
    Ok(())
}

fn self_uninstall(c: &Container, keep_magisk: bool, yes: bool) -> Result<()> {
    utils::msg_main("wsu self-uninstall");
    if !keep_magisk {
        if utils::ask_yes("Also remove KitsuneMagisk from Waydroid?", yes) {
            magisk::remove(c, true)?;
        }
    } else {
        utils::msg_sub("keeping KitsuneMagisk installed as requested");
    }
    let exe = std::env::current_exe()?;
    utils::msg_sub(&format!("removing wsu binary at {}", exe.display()));
    if exe.is_file() {
        let _ = std::fs::remove_file(&exe);
    }
    for p in ["/usr/local/bin/wsu", "/usr/bin/wsu"] {
        if std::path::Path::new(p).is_file() && std::path::Path::new(p) != exe.as_path() {
            let _ = std::fs::remove_file(p);
        }
    }
    utils::msg_ok("wsu removed — bye");
    Ok(())
}
