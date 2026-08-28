# Contributing to GitClean

Thanks for helping make GitClean safer and more useful.

## Principles

Safety wins over cleanup coverage. A false negative leaves some cache behind; a false positive can destroy useful local data.

Do not weaken these rules:

- dry-run is the default,
- deletion requires `--apply`,
- `.git` is untouchable,
- tracked files are untouchable,
- symlinks are never followed,
- dangerous scan roots are refused.

## Development workflow

1. Create a focused feature branch.
2. Add tests before or alongside behavior changes, especially for deletion paths.
3. Keep dependencies minimal and justify any new dependency.
4. Run the full local gate:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

5. Open a pull request describing behavior and safety implications.
6. Merge only after CI passes.

## Adding a generated directory

Prefer names that are strongly associated with generated output. Generic names such as `build`, `dist`, `out`, and `coverage` need extra evidence before deletion; v0.1.0 requires Git to report those directories as ignored.

Add tests showing that tracked content, `.git` metadata, and symlink targets remain protected.

## Scope

GitClean is Linux-first, local-only software. Please do not add telemetry, analytics, accounts, a backend service, or automatic network access.
