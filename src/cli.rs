use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "wsu",
    version,
    about = crate::constants::WSU_TAGLINE,
    long_about = "WaydroidSU (wsu) — a CLI companion that installs and manages KitsuneMagisk\n\
                  (Jordan231111/KitsuneMagisk — Kitsune Hide/SuList + Zygisk) inside Waydroid:\n\
                  root, modules, superuser policy, app management and hide controls."
)]
pub struct Cli {
    /// Assume yes for all prompts
    #[arg(short = 'y', long, global = true)]
    pub yes: bool,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Show magisk, daemon, zygisk, hide and module status
    Status,

    /// Install KitsuneMagisk into Waydroid (downloads the latest kitsune-canary release)
    Install {
        /// Use a local release APK instead of downloading
        #[arg(long)]
        apk: Option<PathBuf>,
        /// Override the target ABI (x86_64, arm64-v8a, x86, armeabi-v7a)
        #[arg(long)]
        abi: Option<String>,
    },

    /// Post-boot setup: verify the daemon, install + pre-grant the manager app
    Setup,

    /// Remove KitsuneMagisk from Waydroid
    Remove,

    /// Update KitsuneMagisk to the latest release from the fork
    Update {
        /// Only check for a new release; do not install
        #[arg(long)]
        check: bool,
    },

    /// Restart the Waydroid container
    Restart,

    /// Manage the Kitsune manager app inside Waydroid
    Manager {
        #[command(subcommand)]
        cmd: ManagerCmd,
    },

    /// Manage magisk modules (install from host zip, URL or repo search)
    Module {
        #[command(subcommand)]
        cmd: ModuleCmd,
    },

    /// Search GitHub for magisk modules
    Repo {
        #[command(subcommand)]
        cmd: RepoCmd,
    },

    /// Kitsune Hide (MagiskHide) control — passthrough to `magisk --hide`
    /// (status|enable|disable|ls|add PKG [PROC]|rm PKG [PROC]|mode hide|sulist|provider core|external|check UID PROC|exec CMD)
    MagiskHide {
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// DenyList control — passthrough to `magisk --denylist`
    /// (status|enable|disable|add PKG [PROC]|rm PKG [PROC]|ls|exec CMD)
    Denylist {
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// SuList mode (allowlist-based hiding) control
    Sulist {
        #[command(subcommand)]
        cmd: SulistCmd,
    },

    /// Zygisk control
    Zygisk {
        #[command(subcommand)]
        cmd: ZygiskCmd,
    },

    /// Superuser policy management (control which apps have root)
    Su {
        #[command(subcommand)]
        cmd: Option<SuCmd>,
        /// run the remaining arguments through `su -c` inside Waydroid
        #[arg(last = true)]
        passthrough: Vec<String>,
    },

    /// Android app management inside Waydroid (pm wrapper)
    App {
        #[command(subcommand)]
        cmd: AppCmd,
    },

    /// Uninstall wsu itself (and optionally KitsuneMagisk)
    SelfUninstall {
        /// keep KitsuneMagisk installed in Waydroid
        #[arg(long)]
        keep_magisk: bool,
    },
}

#[derive(Subcommand)]
pub enum ManagerCmd {
    /// Install (or reinstall) the Kitsune manager app
    Install,
    /// Launch the manager app
    Launch,
}

#[derive(Subcommand)]
pub enum ModuleCmd {
    /// List installed modules
    List,
    /// Install a module from a host zip path, an https URL, or a repo query
    Install {
        /// zip path | zip URL | owner/repo | search query
        source: String,
    },
    /// Remove a module (applied on next boot)
    Remove {
        /// module id (see `wsu module list`)
        id: String,
    },
    /// Enable a module (applied on next boot)
    Enable {
        id: String,
    },
    /// Disable a module (applied on next boot)
    Disable {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum RepoCmd {
    /// Search GitHub for magisk modules
    Search {
        query: String,
    },
}

#[derive(Subcommand)]
pub enum SulistCmd {
    /// Show current hide mode
    Status,
    /// Switch to SuList mode (allowlist hiding; next boot)
    Enable,
    /// Switch back to MagiskHide mode (denylist hiding; next boot)
    Disable,
}

#[derive(Subcommand)]
pub enum ZygiskCmd {
    /// Show the daemon's Zygisk contract
    Status,
    /// Enable Zygisk (needs a restart to apply)
    Enable,
    /// Disable Zygisk (needs a restart to apply)
    Disable,
}

#[derive(Subcommand)]
pub enum SuCmd {
    /// List all root policies (who has root)
    List,
    /// Grant root to a package
    Grant {
        package: String,
    },
    /// Deny root for a package
    Deny {
        package: String,
    },
    /// Revoke a package's root policy (back to ask)
    Revoke {
        package: String,
    },
}

#[derive(Subcommand)]
pub enum AppCmd {
    /// List installed packages
    List {
        /// only third-party (user-installed) apps
        #[arg(long)]
        third_party: bool,
    },
    /// Install an APK from the HOST into Waydroid
    Install {
        apk: PathBuf,
    },
    /// Uninstall a package
    Uninstall {
        package: String,
    },
}
