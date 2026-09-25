# Kerf — Git workflow

- `main` — released, always green. Only receives PRs from `develop`.
- `develop` — integration. Only receives PRs from `feat/*`, `fix/*`, `docs/*`.
- Work happens on short-lived branches off `develop`.
- **No direct merges or pushes to `main`/`develop`.** Every change lands via a GitHub PR (`gh pr create` → review → `gh pr merge --merge`).
- A PR merges only when `cargo build`, `cargo test`, `cargo clippy` pass locally (CI to be added).
- Commits: Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`, `test:`).
