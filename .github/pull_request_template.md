<!--
Thanks for contributing to WaydroidSU (wsu)!
Please fill in the sections below; delete the ones that don't apply.
-->

## Summary

<!-- What does this PR change and why? Link related issues with "Fixes #123". -->

## Type of change

- [ ] `feat` — new functionality
- [ ] `fix` — bug fix
- [ ] `docs` — documentation only
- [ ] `build` / `ci` — packaging, workflows, dependency bumps
- [ ] `refactor` / `test` — no behavior change

## Changes

<!-- Bullet list of the concrete changes. -->

-
-

## How it was tested

<!-- wsu talks to Waydroid from the HOST as root — say what you actually ran. -->

- [ ] `cargo build --all-targets`
- [ ] `cargo test --all-targets`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] Exercised on a real Waydroid setup:
  - Waydroid version:
  - Image (lineage / vanilla, Android version):
  - Distro:
  - Commands tried:

## Checklist

- [ ] Commits follow conventional commits (`feat:`, `fix:`, `docs:`, `build:`, `ci:`, ...)
- [ ] No new warnings from `cargo build --all-targets`
- [ ] Docs updated (README command map / docs/USAGE.md) if commands changed
- [ ] Anything generated (`wsu_boot.sh`, rc blocks) stays idempotent across reboots
