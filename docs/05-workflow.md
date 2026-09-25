# Kerf — Git workflow

- `main` — released, always green. Only receives PRs from `develop`.
- `develop` — integration. Only receives PRs from `feat/*`, `fix/*`, `docs/*`.
- Work happens on short-lived branches off `develop`.
- **No direct merges or pushes to `main`/`develop`.** Every change lands via a GitHub PR (`gh pr create` → review → `gh pr merge --merge`).
- A PR merges only when `cargo build`, `cargo test`, `cargo clippy` pass locally (CI to be added).
- Commits: Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`, `test:`).

## CI / releases

- **CI** (`.github/workflows/ci.yml`) on every PR and on pushes to `main` / `develop`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, release perf tests. macOS runner (Apple Silicon).
- **Release** (`.github/workflows/release.yml`): push a tag `vX.Y.Z` that matches `Cargo.toml` → tests, universal `Kerf.app` (arm64 + x86_64 via `lipo`, ad-hoc signed), zip + `SHA256SUMS.txt`, published to GitHub Releases with install notes. `workflow_dispatch` builds the same zip as a downloadable artifact without releasing.
- To cut a release: bump `version` in `Cargo.toml` → PR → merge to `main` → `git tag v0.2.0 && git push origin v0.2.0`.
- Not yet: notarization (needs an Apple Developer ID), Linux/Windows builds.
