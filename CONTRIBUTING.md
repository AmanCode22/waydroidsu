# Contributing to WaydroidSU (wsu)

Thanks for your interest in making `wsu` better. This document covers building,
testing, packaging and the conventions used in this repository.

## Getting started

Requirements: Rust 1.74+ (via [rustup](https://rustup.rs)), `curl`, and — for
anything beyond unit tests — a machine with [Waydroid](https://waydroid.org) installed.

```bash
git clone https://github.com/AmanCode22/waydroidsu
cd waydroidsu
cargo build            # debug build
cargo test --all-targets
cargo run -- status    # try the CLI (degrades gracefully without waydroid)
```

## Project layout

| Path | What it is |
| --- | --- |
| `src/main.rs` | entry point + `wsu status` implementation |
| `src/cli.rs` | clap command definitions (single source of truth for the CLI) |
| `src/constants.rs` | upstream repo/paths, payload mappings, rc blocks, `wsu_boot.sh` |
| `src/container.rs` | waydroid host interface: `waydroid shell`, system-root resolution, magisk CLI wrapper |
| `src/magisk.rs` | install / setup / update / remove, APK download + payload extraction, rc patching |
| `src/modules.rs` | module list/install/remove/enable/disable + GitHub repo search |
| `src/superuser.rs` | root policies via `magisk --sqlite` (policies table) + `pm` app management |
| `src/hide.rs` | Kitsune Hide / SuList / DenyList / Zygisk control |
| `src/utils.rs` | downloads, GitHub API, zip extraction, file pushing into the container |
| `.github/workflows/release.yml` | builds the binary tarball, `.deb` and `.rpm` on every published release |

## Coding conventions

- Plain Rust 2021, no async: everything is `std::process::Command` + `anyhow::Result`.
- User-facing output goes through the `msg_main / msg_sub / msg_ok / msg_warn / msg_err`
  helpers in `utils.rs`; error paths must degrade to a clean `[X] …` message and exit 1.
- Anything that talks to the container must survive quoting: pass arguments as separate
  argv entries and re-expand them inside `sh -ec '… "$@"'` (see `container.rs`).
- Generated files (`wsu_boot.sh`, rc block) must stay **idempotent** — the container
  re-runs them on every boot.
- Destructive commands go through `ask_yes()` (respects `--yes`).
- CI enforces `cargo clippy --all-targets -- -D warnings`; run it locally before pushing.

## Testing without Waydroid

The unit tests run anywhere (`cargo test`). The APK-extraction test uses a real
`kitsune-canary` release APK if one exists at `/tmp/apk-inspect/kitsune.apk` and skips
otherwise — drop a downloaded release APK there to exercise the extractor end-to-end.
Commands that need a container print a clean error when Waydroid is absent, so
`cargo run -- <cmd>` is safe to smoke-test anywhere.

## Packaging locally

```bash
cargo build --release
cargo install --locked cargo-deb cargo-generate-rpm
cargo deb                                   # → target/debian/*.deb
cargo generate-rpm                          # → target/release/*.rpm
```

Deb/rpm metadata lives in `Cargo.toml` under `[package.metadata.deb]` and
`[package.metadata.generate-rpm]`.

## Commits & pull requests

- Conventional commits: `feat:`, `fix:`, `docs:`, `build:`, `ci:`, `refactor:`, `test:`.
- One logical change per commit; keep `Cargo.lock` in sync when you touch dependencies.
- Open PRs against `main` using the PR template; make sure the CI matrix (stable + 1.74)
  is green.
- Dependabot opens weekly `cargo` and `github-actions` PRs — they follow the same review
  rules; group-bumped minor/patch PRs can be merged without a Waydroid re-test if CI is green.

## Releasing

1. Bump `version` in `Cargo.toml` (commit: `build: release v1.2.3`).
2. Tag and publish a GitHub release (`v1.2.3`).
3. `release.yml` runs on the published release: builds for `x86_64` and `aarch64`,
   attaches `wsu-<ver>-<arch>-linux.tar.gz`, `waydroidsu_<ver>_<arch>.deb`,
   `waydroidsu-<ver>-<arch>.rpm` and `sha256sums.txt` to the release automatically.

## Reporting bugs

Open an issue with: `sudo wsu status` output, Waydroid version + image (Android version),
distro, and the full command output with `-v`-style detail (`sudo wsu <cmd>` as run).
Never paste `magisk.db` contents or app lists that include private packages — trim them.
