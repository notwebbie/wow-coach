# Security Policy

## Supported versions

WoW Coach is pre-release software. Security fixes currently target the latest `main` branch only.

## Reporting a vulnerability

Please use GitHub's private vulnerability reporting for this repository rather than opening a public issue. Include a minimal reproduction, affected revision, and impact. Do **not** include real SavedVariables, account labels, character data, local paths, tokens, or databases.

If private vulnerability reporting is unavailable, open a public issue containing no exploit details or personal data and ask a maintainer for a private contact channel.

## Security boundaries

The addon must remain network-free and must not automate protected combat actions. Desktop SavedVariables parsing must treat files as untrusted data and must not execute Lua. File discovery and import code must reject symlinks and avoid path traversal. SQLite queries must be parameterized.

No project contributor will ask for a complete `WTF` directory, live database, credential, or access token.
