# AGENTS.md

## Build & Test

```bash
cargo test           # Run all tests (114 tests)
cargo build          # Debug build
cargo build --release # Release build
cargo fmt --all -- --check  # Check formatting
cargo clippy --all-targets -- -D warnings  # Lint
```

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for full architecture documentation.

## Conventions

- TDD: all code has tests. Write tests first.
- Error handling: use `thiserror` for library errors, `anyhow` for application errors.
- `cargo fmt` and `cargo clippy` must pass before push.
- No comments unless necessary for clarity.
- Snake case for files, modules match file names.
