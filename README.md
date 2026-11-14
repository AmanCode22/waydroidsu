# WaydroidSU (wsu) v2

[![CI](https://github.com/AmanCode22/waydroidsu/actions/workflows/ci.yml/badge.svg)](https://github.com/AmanCode22/waydroidsu/actions/workflows/ci.yml)
[![Release build](https://github.com/AmanCode22/waydroidsu/actions/workflows/release.yml/badge.svg)](https://github.com/AmanCode22/waydroidsu/actions/workflows/release.yml)

**A CLI companion that installs and manages KitsuneMagisk inside Waydroid — root, Zygisk, Kitsune Hide/SuList, modules and superuser policy, everything from the host terminal.**

This is a rebuild of the dead `WaydroidSU` project (mistrmochov/WaydroidSU, forks deleted), re-sourced to the actively maintained fork **[Jordan231111/KitsuneMagisk](https://github.com/Jordan231111/KitsuneMagisk)** (Magisk v31.0 base, `kitsune-canary-*` releases).

## Why this fork

| Requirement | Status in Jordan231111/KitsuneMagisk |
| --- | --- |
| Zygisk | ✅ `native/src/core/zygisk/` — and the fork **fixed Zygisk for Waydroid-shaped environments** (`selinux.rs`, `zygisk/mod.rs`: missing `selinuxfs`, AppArmor contexts, companion-socket `fgetxattr` EOPNOTSUPP). Its device-qualification records cover a Waydroid 1.6.2 / Android 13 lifecycle. |
| MagiskHide (the deleted fork had it) | ✅ Kitsune Hide with a first-party CLI: `magisk --hide status/enable/disable/ls/add/rm`, plus SuList allowlist mode (`--hide mode sulist`) and DenyList (`--denylist`). |
| Fresh | ✅ Canary releases published continuously; last release Oct 2026 (within the last month). |
| Modules | ✅ `magisk --install-module`, plus `/data/adb/modules` compatibility. |

The old WSU's install target (`/system/etc/init/magisk` + `bootanim.rc` injection) is still the route the fork's System Mode transaction engine explicitly understands (`bootanim` / `bootanim_gz` labels), so `wsu` stays compatible with it.

## How it works

1. `wsu install` downloads the latest `kitsune-canary` `app-release.apk` from the fork's GitHub releases (or uses `--apk /path/to.apk`).
2. The APK's native libraries are extracted into Waydroid's writable system tree
   (`/var/lib/waydroid/overlay/system/etc/init/magisk/` when overlays are enabled, else `rootfs/system/...`):
   `libmagisk.so → magisk`, `libmagiskinit.so → magiskinit`, `libmagiskpolicy.so → magiskpolicy`,
   `libmagiskboot.so → magiskboot`, `libbusybox.so → busybox`, plus `init-ld`, `bootctl` and the
   32-bit `magisk32` companion when the image supports 32-bit ABIs. Script assets land there too.
3. A generated bootstrap script (`wsu_boot.sh`, modeled on the fork's own `scripts/live_setup.sh`)
   stages the payload into `/data/adb/magisk`, sets up `/sbin` (su/resetprop/supolicy links, worker
   tmpfs) and runs the daemon stages (`--post-fs-data`, `--service`, `--boot-complete`, `--zygote-restart`).
4. `bootanim.rc` gets a clearly-marked wsu block appended (with an automatic `.wsu-orig` backup;
   `bootanim.rc.gz` images are handled by decompressing first) that runs the bootstrap at
   `post-fs-data` **synchronously via `exec`**, so the daemon is always up before zygote starts —
   that is what makes Zygisk injection work.
5. `wsu setup` verifies the daemon, installs the Kitsune manager app into the container and
   pre-grants it root.

## Install

**From a GitHub release (recommended)** — every published release gets a prebuilt
binary tarball for `x86_64` and `aarch64`, plus `.deb` / `.rpm` packages and a
`sha256sums.txt`, built by [release.yml](.github/workflows/release.yml):

```bash
# binary tarball
tar -xzf wsu-*-x86_64-linux.tar.gz && sudo install -m755 wsu-*/wsu /usr/local/bin/wsu

# Debian / Ubuntu / Debian-based Waydroid distros
sudo apt install ./waydroidsu_*_amd64.deb

# Fedora / openSUSE / RHEL-based
sudo dnf install ./waydroidsu-*_64bit.rpm   # naming: waydroidsu-<ver>-1.x86_64.rpm
```

**From source** (needs Rust 1.88+, see install.sh):

```bash
sudo ./install.sh
# or
cargo build --release && sudo make install
```

Runtime dependencies: `waydroid`, `curl`, `chmod/chown` (coreutils), `base64` (fallback path). Host root for install/update/module operations (`sudo wsu ...`).

## Quick start

```bash
sudo wsu install          # fetch + install KitsuneMagisk into Waydroid
waydroid show-full-ui     # boot Waydroid (or: wsu restart)
sudo wsu setup            # verify daemon, install manager app, pre-grant root
sudo wsu status           # everything at a glance
```

## Command map

| Area | Commands |
| --- | --- |
| Lifecycle | `wsu status` · `install [--apk P] [--abi A]` · `setup` · `remove` · `update [--check]` · `restart` · `self-uninstall [--keep-magisk]` |
| Modules | `wsu module list` · `install <host.zip\|https URL\|owner/repo\|search query>` · `remove ID` · `enable ID` · `disable ID` |
| Repo | `wsu repo search <query>` (GitHub search) |
| Root policy | `wsu su list` · `su grant PKG` · `su deny PKG` · `su revoke PKG` · `su -- <cmd...>` (su passthrough) |
| Apps | `wsu app list [--third-party]` · `app install host.apk` (installs from the host!) · `app uninstall PKG` |
| Hide | `wsu magiskhide` (status summary) · `magiskhide -- enable` · `magiskhide add PKG [PROC]` · `magiskhide rm PKG` · `magiskhide -- mode sulist` |
| SuList | `wsu sulist status` · `sulist enable` · `sulist disable` |
| DenyList | `wsu denylist -- status/enable/disable/add/rm/ls` |
| Zygisk | `wsu zygisk status` · `zygisk enable` · `zygisk disable` |
| Manager | `wsu manager install` · `manager launch` |

Full reference with examples: [docs/USAGE.md](docs/USAGE.md).

## Zygisk notes for Waydroid

- Use an **Android 11–13** Waydroid image for reliable Zygisk. The fork's own qualification matrix
  covers Waydroid 1.6.2 / Android 13 (arm64). Android 16 images are known-broken for Zygisk on
  Waydroid (see waydroid issue #2433) — root still works there, Zygisk modules may not.
- SELinux is expected to be **disabled/permissive** in the container (Waydroid default; the fork's
  Waydroid fixes explicitly target that state).
- Zygisk toggles (`wsu zygisk enable/disable`) write the `zygisk` key in `magisk.db`; they apply
  after `sudo wsu restart`.

## Removal

```bash
sudo wsu remove            # strips magisk from Waydroid, restores bootanim.rc, wipes /data/adb state
sudo wsu self-uninstall    # the above (interactive) + deletes the wsu binary itself
sudo wsu self-uninstall --keep-magisk   # only remove the CLI
```

## Credits & license

- KitsuneMagisk payload: [Jordan231111/KitsuneMagisk](https://github.com/Jordan231111/KitsuneMagisk) (GPL-3.0, continuation of HuskyDG's Kitsune Mask, upstream Magisk by topjohnwu).
- Original WaydroidSU concept: mistrmochov (repo deleted), archived fork samos667/WaydroidSU (Rust, last push 2025-07).
- WaydroidSU v2 is GPL-3.0. This tool only redistributes nothing from the fork at rest — APKs are fetched from the fork's official GitHub releases at install time.
