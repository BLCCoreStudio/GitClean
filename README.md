# GitClean

**Safely find and clean generated build and cache directories.**

GitClean is a small, Linux-first Rust CLI that finds generated build and cache directories in developer projects and tells you how much space they are likely using.

**Dry-run is the default. GitClean never deletes anything unless you explicitly pass `--apply`.**

## Status

**GitClean v0.1.0 is available as the first public release.**

A prebuilt Linux x86_64 archive and SHA-256 checksum are available on the [GitHub Releases page](https://github.com/BLCCoreStudio/GitClean/releases/tag/v0.1.0).

## Why

Build outputs and tool caches quietly accumulate across repositories. Commands such as broad recursive deletes are fast, but they are also easy to aim at the wrong path. GitClean takes the conservative route: detect known generated directories, protect Git-tracked files, refuse dangerous targets, and require an explicit apply flag before deletion.

## v0.1.0 behavior

```text
gitclean .
```

Scans the project and prints candidates plus an estimated reclaimable size. It does **not** delete files.

```text
gitclean . --apply
```

Deletes only candidates that pass every safety check.

Known generated directory names include:

- Rust: `target/`
- Node / frontend: `node_modules/`, `.next/`, `.nuxt/`, `.parcel-cache/`, `.turbo/`, `.vite/`
- Python: `__pycache__/`, `.pytest_cache/`, `.mypy_cache/`, `.ruff_cache/`, `.tox/`, `.nox/`
- Gradle: `.gradle/`
- CMake: `cmake-build-*`
- Common outputs: `build/`, `dist/`, `out/`, `coverage/`

The common output names are more ambiguous, so GitClean only considers them safe for `--apply` when Git also reports the directory as ignored.

## Safety model

GitClean is intentionally conservative:

- Dry-run by default; deletion requires explicit `--apply`.
- `--apply` requires a Git worktree so tracked-file protection is available.
- A candidate containing any Git-tracked file is skipped entirely.
- `.git` is never traversed or deleted. A candidate containing `.git` metadata is skipped entirely.
- Symlinks are never followed. A candidate that is itself a symlink is skipped.
- Filesystem root, the current user's home directory, paths inside `.git`, and symlink targets supplied as the scan root are refused.
- Candidates are revalidated immediately before deletion, including tracked-file and `.git` checks.
- No telemetry, network calls, backend, account, or analytics exist. GitClean only invokes the local `git` executable for safety checks.

GitClean reduces common cleanup mistakes, but `--apply` still deletes local untracked generated data. Review the dry-run output first.

## Install on Linux x86_64

Download these files from the [v0.1.0 release](https://github.com/BLCCoreStudio/GitClean/releases/tag/v0.1.0):

- `gitclean-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`
- `gitclean-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256`

Verify and extract:

```bash
sha256sum -c gitclean-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf gitclean-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
./gitclean --version
```

You can optionally place `gitclean` somewhere on your `PATH`, such as `~/.local/bin`.

## Build from source

Requires stable Rust and Git.

```bash
git clone https://github.com/BLCCoreStudio/GitClean.git
cd GitClean
cargo build --release --locked
./target/release/gitclean .
```

For local installation:

```bash
cargo install --path . --locked
```

## Example

```text
$ gitclean .
GitClean dry-run — nothing will be deleted
Target: /home/alice/code/example
SAFE    84.2 MiB  node_modules
SAFE    11.8 MiB  target
SKIP     2.1 MiB  dist — ambiguous directory name and not ignored by Git

Reclaimable: 96.0 MiB across 2 safe directories
Run again with --apply to delete only SAFE entries.
```

## Development

Before submitting a change:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).

## License

MIT © BLC Core Studio