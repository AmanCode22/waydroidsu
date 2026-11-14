#!/usr/bin/env bash
# WaydroidSU (wsu) installer — builds from source and installs the wsu binary.
# Needs: cargo (Rust 1.74+). Run: sudo ./install.sh
set -euo pipefail

if ! command -v cargo >/dev/null 2>&1; then
  echo "!! cargo not found — install Rust first: https://rustup.rs (curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh)"
  exit 1
fi

echo "=> building wsu (release)"
cargo build --release

if [ "$(id -u)" != "0" ]; then
  echo "!! not root; install to ~/.local/bin instead? [y/N]"
  read -r ans
  case "$ans" in
    y|Y)
      install -Dm755 target/release/wsu "$HOME/.local/bin/wsu"
      echo "=> installed: $HOME/.local/bin/wsu (make sure ~/.local/bin is on PATH)"
      ;;
    *) echo "aborted"; exit 1 ;;
  esac
else
  install -Dm755 target/release/wsu /usr/local/bin/wsu
  echo "=> installed: /usr/local/bin/wsu"
fi

cat <<'EOF'

WaydroidSU (wsu) installed. Next steps:

  1. sudo wsu install      # downloads KitsuneMagisk (Jordan231111/KitsuneMagisk canary) into Waydroid
  2. waydroid show-full-ui # boot Waydroid
  3. sudo wsu setup        # verify daemon, install + pre-grant the manager app
  4. sudo wsu status       # check zygisk / hide / modules

Help: wsu --help   |   Full docs: docs/USAGE.md
EOF
