# Release Tagging Procedure — GRPC-Tauri

## Release Procedure

1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`
4. `bun run check`
5. `bun run check:arch`
6. `git status` must be clean
7. `git tag -a X.Y.Z -m "Release vX.Y.Z"`
8. `git push origin main --tags`

## Certification Rule

No release tag may be created if:

* governance violations exist
* architecture violations exist
* tests fail
* certification documents are missing
