# wsu — command reference

`wsu` runs on the **host**, not inside Waydroid. Commands that install, modify or
inspect the container need root: prefix with `sudo`. Add `--yes` / `-y` anywhere to
auto-answer prompts.

Environment: `GITHUB_TOKEN` (optional) is used for GitHub API calls (release metadata,
repo search) and avoids rate limits.

## Lifecycle

### `wsu status`
One screen with everything: wsu version, upstream fork, Waydroid container/session
state, overlay mode, installed KitsuneMagisk release, daemon version (`magisk -c`),
capabilities (`--features`), zygisk/sulist settings, module count and manager app.

```bash
sudo wsu status
```

### `wsu install [--apk /path/app-release.apk] [--abi x86_64]`
The main event:
1. Downloads the latest `app-release.apk` from [Jordan231111/KitsuneMagisk releases](https://github.com/Jordan231111/KitsuneMagisk/releases)
   (or uses `--apk`; a cached copy is offered for reuse).
2. Extracts the native payload for the detected ABI into
   `/var/lib/waydroid/overlay/system/etc/init/magisk/` (overlay mode) or
   `rootfs/system/etc/init/magisk/` — `libmagisk.so → magisk`, plus
   `magiskinit`, `magiskpolicy`, `magiskboot`, `busybox`, `init-ld`, `bootctl`,
   script assets, and `magisk32` when the image supports 32-bit ABIs.
3. Writes `wsu_boot.sh` and appends the wsu bootstrap block to `bootanim.rc`
   (with a `.wsu-orig` backup; `.gz` bootanim files are decompressed first).
4. Stops a running container first (asks, unless `-y`).
5. Records state in `/var/lib/waydroid/wsu/wsu.json`.

```bash
sudo wsu install
sudo wsu install --apk ~/Downloads/app-release.apk
sudo wsu install --abi arm64-v8a     # when auto-detection is wrong
```

### `wsu setup`
Run **after booting Waydroid** once (`waydroid show-full-ui` or `wsu restart`):
waits for `sys.boot_completed`, verifies `/sbin/magisk` and the daemon, prints
capabilities + zygisk status, installs the Kitsune manager app (from the cached APK)
and pre-grants it root so the first run asks nothing.

```bash
sudo wsu setup
```

### `wsu update [--check]`
Fetches the fork's latest release and compares with the installed tag.
`--check` only reports (works without Waydroid); without it, re-runs the installer
and drops the cached APK. The daemon self-updates `/data/adb/magisk` from the
payload on the next boot.

```bash
sudo wsu update --check
sudo wsu update
```

### `wsu remove`
Stops the daemon and container, deletes the payload directory, restores the original
`bootanim.rc` from its backup, wipes `/data/adb/magisk`, `/data/adb/modules` and the
wsu state file. Asks first.

### `wsu restart`
`waydroid session stop` + `container stop` + `container start` — the clean way to
apply zygisk/sulist/module changes.

### `wsu self-uninstall [--keep-magisk]`
Removes KitsuneMagisk (unless `--keep-magisk`) and deletes the `wsu` binary itself.

## Modules

```bash
sudo wsu module list                          # id, name, version, author + flags
sudo wsu module install ~/Downloads/module.zip     # host path
sudo wsu module install https://example.com/mod.zip # URL
sudo wsu module install Tom60703/MagiskHideProps   # owner/repo (picks its release zip)
sudo wsu module install "shamrock zygisk"          # free-text repo search → best hit
sudo wsu module disable zygisk_assistant
sudo wsu module enable  zygisk_assistant
sudo wsu module remove  zygisk_assistant           # applied on next boot
```

Install runs the fork's own installer through `magisk --install-module` after pushing
the zip into the container (stdin pipe with base64-chunk fallback).

## Repo search

```bash
wsu repo search zygisk      # GitHub search over magisk modules
```

## Root policy (`wsu su`)

Reads/writes the daemon's `policies` table via `magisk --sqlite`
(`0` ask, `1` deny, `2` allow):

```bash
sudo wsu su list                 # uid, package, policy, notifications
sudo wsu su grant com.termux     # allow root
sudo wsu su deny  com.bank.app   # deny + hide from requests
sudo wsu su revoke com.termux    # back to ask-on-use
```

`wsu su -- <cmd…>` is a `su -c` passthrough inside Waydroid:

```bash
sudo wsu su -- id
sudo wsu su -- ls /data/adb
```

## Apps (`wsu app`)

```bash
sudo wsu app list                    # all packages
sudo wsu app list --third-party      # user-installed only
sudo wsu app install ~/host.apk      # pushes + pm installs from the HOST
sudo wsu app uninstall com.example   # pm uninstall
```

## Kitsune Hide / SuList / DenyList

`wsu magiskhide` with no args prints a status summary (hide enforcement, denylist,
mode, zygisk). Everything else is a passthrough to the fork's first-party CLI:

```bash
sudo wsu magiskhide -- enable              # enforce Kitsune Hide
sudo wsu magiskhide -- disable
sudo wsu magiskhide add  com.bank.app      # hide package (all its processes)
sudo wsu magiskhide add  com.bank.app com.bank.app.ui   # single process
sudo wsu magiskhide rm   com.bank.app
sudo wsu magiskhide ls
sudo wsu magiskhide -- mode sulist         # switch engine (next boot) — or: wsu sulist enable
sudo wsu magiskhide -- mode hide
sudo wsu magiskhide -- provider external   # external mount provider (next boot)
```

SuList (allowlist hiding — hide everything NOT listed):

```bash
sudo wsu sulist status     # current mode
sudo wsu sulist enable     # next boot
sudo wsu sulist disable    # back to MagiskHide mode
```

DenyList passthrough (`status|enable|disable|add|rm|ls|exec`):

```bash
sudo wsu denylist -- enable
sudo wsu denylist add com.bank.app
sudo wsu denylist ls
```

Flags that start with `-` need the `--` separator, as shown.

## Zygisk

```bash
sudo wsu zygisk status     # daemon's zygisk contract + db setting
sudo wsu zygisk enable     # writes zygisk=1 into magisk.db — apply with sudo wsu restart
sudo wsu zygisk disable
```

## Manager app

```bash
sudo wsu manager install   # download/reuse APK → pm install → pre-grant root
sudo wsu manager launch    # waydroid app launch io.github.jordan231111.magisk
```

## Exit codes

`0` success · `1` any failure (message printed as `[X] …` on stderr).

## Typical first-run flow

```bash
sudo wsu install
sudo wsu restart           # or: waydroid show-full-ui
sudo wsu setup
sudo wsu zygisk enable
sudo wsu magiskhide add com.bank.app
sudo wsu restart           # apply hide + zygisk
```
