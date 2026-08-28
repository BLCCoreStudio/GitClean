# Security Policy

GitClean deletes local directories only when `--apply` is explicitly supplied, so deletion-safety bugs are treated as security-sensitive.

## Supported versions

GitClean is currently pre-release software. Security fixes are made on the latest `main` branch until a stable release policy is established.

## Reporting a vulnerability

Please do not publish a destructive proof of concept against real user data.

Use GitHub's private vulnerability reporting feature for this repository if it is available. If private reporting is unavailable, open a minimal issue that states a security problem exists without including exploit details, destructive commands, private paths, tokens, or user data. A maintainer can then arrange a private channel.

Useful reports include:

- the GitClean commit or version,
- operating system and filesystem,
- the exact directory layout needed to reproduce the issue using disposable test data,
- whether symlinks, nested repositories, worktrees, tracked files, or concurrent filesystem changes are involved,
- expected versus actual behavior.

## Security invariants

Changes must preserve these invariants:

1. No deletion without explicit `--apply`.
2. Never traverse or delete `.git` metadata.
3. Never delete Git-tracked files.
4. Never follow symlinks during discovery or size calculation.
5. Refuse filesystem root, the current user's home directory, paths inside `.git`, and a symlink supplied as the scan root.
6. Revalidate deletion candidates immediately before deletion.
7. No telemetry, network service, backend, or remote execution behavior.

Any pull request that changes deletion logic should add or strengthen tests for these invariants.
