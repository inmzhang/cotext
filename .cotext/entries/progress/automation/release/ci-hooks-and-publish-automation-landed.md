---
id: ci-hooks-and-publish-automation-landed
title: CI, hooks, and publish automation landed
category: progress
section: automation/release
status: active
tags:
- ci
- docs
- release
created_at: 2026-03-10T08:45:51.931466714Z
updated_at: 2026-03-10T14:27:15.044661566Z
---
Completed
- Added GitHub Actions CI for `cargo fmt --all -- --check`, `cargo check --all-targets --all-features`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`.
- Added a tracked `.pre-commit-config.yaml` with the same Rust quality gates and installed the local pre-commit hook for this checkout.
- Added a crates.io publish workflow with release-tag/version checks and `CARGO_REGISTRY_TOKEN` support, plus package metadata and README install/development guidance.
- Set the crate license metadata to `MIT`, which clears the last crates.io packaging warning.

Validation
- `cargo fmt --all -- --check` passes.
- `cargo check --all-targets --all-features` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- `cargo test` passes.
- `cargo publish --dry-run --allow-dirty` passes.
- `pre-commit run --all-files` passes.

Next step
- Set the `CARGO_REGISTRY_TOKEN` GitHub secret, then publish from a matching release tag or `workflow_dispatch`.

Release
- Published the GitHub release `v0.1.0` with release notes via `gh release create v0.1.0`.
- Confirmed the `Publish` GitHub Actions workflow started from the `release` event: https://github.com/inmzhang/cotext/actions/runs/22895651383

Docs
- Shortened the published README and switched the install section to the direct `cargo install cotext` guidance now that the crate is live.

Release
- Published the GitHub release `v0.1.1` with release notes via `gh release create v0.1.1 --title "v0.1.1" --generate-notes --latest` on commit `7c5dc1961f5a3152b6ed7fef1854e1c4b533ac90`: https://github.com/inmzhang/cotext/releases/tag/v0.1.1
- Confirmed the `Publish` GitHub Actions workflow started from the `release` event for `v0.1.1`: https://github.com/inmzhang/cotext/actions/runs/22907419098

Docs
- Removed the README development section so the public README stays focused on install, commands, agent integration, and TUI usage.
