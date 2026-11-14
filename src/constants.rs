// Constants for WaydroidSU (wsu).
// Sourced to the maintained KitsuneMagisk fork: Jordan231111/KitsuneMagisk
// (next-system branch, Magisk v31.0 base, kitsune-canary-* releases).

pub const WSU_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const WSU_TAGLINE: &str = "WaydroidSU — KitsuneMagisk (Zygisk) manager for Waydroid";

/// Maintained KitsuneMagisk fork providing Zygisk + Kitsune Hide/SuList.
pub const KITSUNE_REPO: &str = "Jordan231111/KitsuneMagisk";
pub const GITHUB_API: &str = "https://api.github.com";
pub const RAW_GH: &str = "https://raw.githubusercontent.com";

/// The manager app packaged inside the Kitsune release APK (application id
/// verified from the fork's device qualification records).
pub const MANAGER_PKG: &str = "io.github.jordan231111.magisk";
pub const APK_ASSET: &str = "app-release.apk";

pub const WAYDROID_DIR: &str = "/var/lib/waydroid";
pub const WAYDROID_CFG: &str = "/var/lib/waydroid/waydroid.cfg";
pub const OVERLAY_SYSTEM: &str = "/var/lib/waydroid/overlay/system";
pub const ROOTFS_SYSTEM: &str = "/var/lib/waydroid/rootfs/system";
pub const WSU_STATE_DIR: &str = "/var/lib/waydroid/wsu";

/// magisk payload dir, relative to the system root (overlay/system or rootfs/system).
pub const MAGISK_DIR_REL: &str = "etc/init/magisk";
pub const BOOTANIM_RC_REL: &str = "etc/init/bootanim.rc";
pub const BOOTANIM_RC_GZ_REL: &str = "etc/init/bootanim.rc.gz";
pub const RC_BACKUP_SUFFIX: &str = ".wsu-orig";

/// In-container paths.
pub const CONTAINER_MAGISKBIN: &str = "/data/adb/magisk";
pub const CONTAINER_MODULES_DIR: &str = "/data/adb/modules";
pub const CONTAINER_TMP: &str = "/data/local/tmp";

/// Where wsu stages the payload INSIDE the container (via the host overlay).
pub const CONTAINER_PAYLOAD: &str = "/system/etc/init/magisk";

/// APK lib dir -> payload binary mapping (verified against a real
/// kitsune-canary app-release.apk built from this fork).
pub const PAYLOAD_LIBS: &[(&str, &str)] = &[
    ("libmagisk.so", "magisk"),
    ("libmagisk32.so", "magisk32"),
    ("libmagiskinit.so", "magiskinit"),
    ("libmagiskpolicy.so", "magiskpolicy"),
    ("libmagiskboot.so", "magiskboot"),
    ("libbusybox.so", "busybox"),
    ("libinit-ld.so", "init-ld"),
    ("libbootctl.so", "bootctl"),
];

/// Magisk policies table values (Magisk v31, su/db.rs + core SuPolicy enum):
/// 0 = query/ask, 1 = deny, 2 = allow.
pub const POLICY_ALLOW: i64 = 2;
pub const POLICY_DENY: i64 = 1;

/// Android init rc block appended to bootanim.rc by `wsu install`.
/// Schema mirrors the fork's System Mode rc (scripts/kitsune_system_install.sh):
/// the prepare stage runs as u:r:init:s0 (always defined) and applies
/// magiskpolicy, which creates the magisk domain; the daemon stages then run
/// as u:r:magisk:s0. The previous u:r:su:s0 labels do not exist in the
/// waydroid GSI policy and init silently refused every exec.
pub const RC_BLOCK_BEGIN: &str = "# >>> wsu (WaydroidSU) KitsuneMagisk bootstrap >>>";
pub const RC_BLOCK_END: &str = "# <<< wsu (WaydroidSU) KitsuneMagisk bootstrap <<<";

pub const RC_BLOCK: &str = r#"# >>> wsu (WaydroidSU) KitsuneMagisk bootstrap >>>
on post-fs-data
    exec u:r:init:s0 0 0 -- /system/bin/sh /system/etc/init/magisk/wsu_boot.sh prepare
    exec u:r:magisk:s0 0 0 -- /system/bin/sh /system/etc/init/magisk/wsu_boot.sh post-fs-data

on property:sys.boot_completed=1
    exec u:r:magisk:s0 0 0 -- /system/bin/sh /system/etc/init/magisk/wsu_boot.sh service
    exec u:r:magisk:s0 0 0 -- /system/bin/sh /system/etc/init/magisk/wsu_boot.sh boot-complete

on property:init.svc.zygote=restarting
    exec u:r:magisk:s0 0 0 -- /system/bin/sh /system/etc/init/magisk/wsu_boot.sh zygote-restart
# <<< wsu (WaydroidSU) KitsuneMagisk bootstrap <<<
"#;

/// Fallback bootanim.rc used only when the image ships no bootanim.rc(.gz):
/// standard AOSP bootanimation service + the wsu bootstrap block.
pub const BOOTANIM_RC_FALLBACK: &str = r#"service bootanim /system/bin/bootanimation
    class core animation
    user graphics
    group graphics audio
    disabled
    oneshot
    ioprio rt 0
    task_profiles MaxPerformance

# >>> wsu (WaydroidSU) KitsuneMagisk bootstrap >>>
on post-fs-data
    exec u:r:init:s0 0 0 -- /system/bin/sh /system/etc/init/magisk/wsu_boot.sh prepare
    exec u:r:magisk:s0 0 0 -- /system/bin/sh /system/etc/init/magisk/wsu_boot.sh post-fs-data

on property:sys.boot_completed=1
    exec u:r:magisk:s0 0 0 -- /system/bin/sh /system/etc/init/magisk/wsu_boot.sh service
    exec u:r:magisk:s0 0 0 -- /system/bin/sh /system/etc/init/magisk/wsu_boot.sh boot-complete

on property:init.svc.zygote=restarting
    exec u:r:magisk:s0 0 0 -- /system/bin/sh /system/etc/init/magisk/wsu_boot.sh zygote-restart
# <<< wsu (WaydroidSU) KitsuneMagisk bootstrap <<<
"#;

/// Boot bootstrap executed inside the container by the injected rc block
/// (and directly by `wsu setup` when the rc triggers did not fire).
/// Mirrors the fork's supported flow (scripts/live_setup.sh + the System
/// Mode launcher) for writable targets:
///   1. stage the payload into /data/adb/magisk (MAGISKBIN)
///   2. tmpfs-mount the runtime — /sbin when the image has one,
///      /debug_ramdisk otherwise (Q+ images ship no /sbin and a read-only
///      root, so mkdir /sbin alone cannot work)
///   3. copy binaries, create .magisk dirs, run --preinit-device
///   4. apply magiskpolicy --live --magisk (creates the magisk SELinux
///      domain that the rc block's u:r:magisk:s0 execs need)
///   5. run the requested daemon stage
/// Idempotent — safe to run on every boot.
pub const WSU_BOOT_SH: &str = r#"#!/system/bin/sh
# WaydroidSU (wsu) KitsuneMagisk boot bootstrap — generated by wsu, do not edit.
MD=/system/etc/init/magisk
ADB=/data/adb/magisk
stage="$1"

[ -x "$MD/magisk" ] || exit 0

wsu_log() { log -t wsu -- "$1" 2>/dev/null || echo "$1"; }

# Runtime dir: mirror live_setup.sh's layout handling.
if [ -x /sbin/magisk ]; then
  MT=/sbin
elif [ -x /debug_ramdisk/magisk ]; then
  MT=/debug_ramdisk
elif [ -d /sbin ] && [ ! -L /sbin ]; then
  MT=/sbin
else
  MT=/debug_ramdisk
  [ -d "$MT" ] || mkdir -p "$MT" 2>/dev/null
fi
[ -L "$MT" ] && { wsu_log "$MT is a symlink"; exit 1; }

# --- stage payload into /data/adb/magisk ---
stage_bin() {
  mkdir -p "$ADB" /data/adb/modules /data/adb/post-fs-data.d /data/adb/service.d 2>/dev/null
  for f in magisk magisk32 magiskpolicy magiskinit magiskboot busybox init-ld \
           stub.apk util_functions.sh module_installer.sh boot_patch.sh \
           flash_script.sh uninstaller.sh update_binary.sh addon.d.sh app_functions.sh; do
    [ -f "$MD/$f" ] || continue
    cmp -s "$MD/$f" "$ADB/$f" || { cp -af "$MD/$f" "$ADB/$f" && chmod 0755 "$ADB/$f"; }
  done
}

# --- prepare the tmpfs runtime ---
do_prepare() {
  [ -x "$MT/magisk" ] && [ -f "$MT/.wsu-live" ] && return 0
  if ! grep -q " $MT " /proc/mounts 2>/dev/null; then
    mount -t tmpfs -o 'mode=0755' magisk "$MT" 2>/dev/null || return 1
  fi
  if [ ! -x "$MT/magisk" ]; then
    for f in magisk magiskpolicy busybox; do
      [ -f "$MD/$f" ] || return 1
      cp -af "$MD/$f" "$MT/$f" || return 1
      chmod 0755 "$MT/$f"
    done
    for f in magisk32 stub.apk; do
      [ -f "$MD/$f" ] || continue
      cp -af "$MD/$f" "$MT/$f" && chmod 0755 "$MT/$f"
    done
    ln -sf ./magisk "$MT/su"
    ln -sf ./magisk "$MT/resetprop"
    ln -sf ./magiskpolicy "$MT/supolicy"
  fi
  mkdir -p "$MT/.magisk/device" "$MT/.magisk/worker" || return 1
  # Traversable socket dirs so app UIDs can reach the daemon's IPC.
  chmod 0711 "$MT/.magisk" "$MT/.magisk/device" 2>/dev/null
  if ! grep -q " $MT/.magisk/worker " /proc/mounts 2>/dev/null; then
    mount -t tmpfs -o 'mode=0755' magisk "$MT/.magisk/worker" 2>/dev/null
    mount --make-private "$MT/.magisk/worker" 2>/dev/null
  fi
  : > "$MT/.magisk/config" 2>/dev/null
  : > "$MT/.wsu-live"
  chmod 0711 "$MT" 2>/dev/null
  if [ -d /sys/fs/selinux ]; then
    chcon u:object_r:rootfs:s0 "$MT" 2>/dev/null
    chcon -R u:object_r:system_file:s0 "$MT/magisk" "$MT/magiskpolicy" \
          "$MT/busybox" "$MT/.magisk" 2>/dev/null
  fi
  export MAGISKTMP="$MT"
  # rc 1 = no eligible pre-init partition; live setups continue without rules.
  MAKEDEV=1 "$MT/magisk" --preinit-device >/dev/null 2>&1
  return 0
}

# --- SELinux: inject the magisk domain into the RUNNING policy ---
do_policy() {
  [ -d /sys/fs/selinux ] || return 0
  RULE=""
  if [ -f "$MT/.magisk/preinit/sepolicy.rule" ] &&
     [ ! -e "$MT/.magisk/preinit/kitsune-v2/disabled" ]; then
    RULE="--apply $MT/.magisk/preinit/sepolicy.rule"
  fi
  MP="$ADB/magiskpolicy"
  [ -x "$MP" ] || MP="$MD/magiskpolicy"
  "$MP" --live --magisk $RULE >/dev/null 2>&1
  return 0
}

case "$stage" in
  prepare)
    stage_bin
    do_prepare || { wsu_log "runtime prepare failed for $MT"; exit 1; }
    do_policy
    ;;
  post-fs-data|service|boot-complete|zygote-restart)
    stage_bin
    do_prepare || { wsu_log "runtime prepare failed for $MT"; exit 1; }
    do_policy
    export MAGISKTMP="$MT"
    [ -x "$MT/magisk" ] || exit 1
    if command -v timeout >/dev/null 2>&1; then
      timeout 120 "$MT/magisk" --"$stage"
    else
      "$MT/magisk" --"$stage"
    fi
    ;;
  *) exit 0 ;;
esac
"#;
