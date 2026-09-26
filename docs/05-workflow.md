# Kerf — Git workflow

- `main` — released, always green. Only receives PRs from `develop`.
- `develop` — integration. Only receives PRs from `feat/*`, `fix/*`, `docs/*`.
- Work happens on short-lived branches off `develop`.
- **No direct merges or pushes to `main`/`develop`.** Every change lands via a GitHub PR (`gh pr create` → review → `gh pr merge --merge`).
- A PR merges only when `cargo build`, `cargo test`, `cargo clippy` pass locally (CI to be added).
- Commits: Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`, `test:`).

## CI / releases

- **CI** (`.github/workflows/ci.yml`) on every PR and on pushes to `main` / `develop`, on **macOS, Linux and Windows**: `cargo clippy --all-targets -- -D warnings`, `cargo test` (all three); `cargo fmt --check` and release perf tests on macOS.
- **Release** (`.github/workflows/release.yml`): push a tag `vX.Y.Z` that matches `Cargo.toml` → per-platform tests and packages — universal `Kerf.app` zip (macOS), `tar.gz` with installer + desktop entry (Linux), `zip` with icon-embedded `kerf.exe` (Windows) — then one job publishes them with `SHA256SUMS.txt` and install notes. `workflow_dispatch` builds the artifacts without releasing.
- To cut a release: bump `version` in `Cargo.toml` → PR → merge to `main` → `git tag v0.2.0 && git push origin v0.2.0`.
- Not yet: macOS notarization (Apple Developer ID), Windows code signing, ARM Linux/Windows builds.
