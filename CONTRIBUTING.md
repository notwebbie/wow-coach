# Contributing

Thanks for helping build WoW Coach.

## Before opening a change

1. Keep Anniversary/TBC Classic and local-first behavior as the primary path.
2. Do not add real SavedVariables, reports, databases, account labels, character names, local paths, secrets, binaries, or archives.
3. Use generic fixtures such as `Examplemage`, `Example Realm`, and `EXAMPLE-ACCOUNT`.
4. Keep product claims aligned with working code.

## Quality checks

```sh
cargo fmt --all --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
(cd desktop && npm ci && npm run build)
./scripts/validate-addon.sh
./scripts/package-addon.sh test
unzip -l dist/WoWCoachCollector-test.zip
```

If `luac` is installed, addon validation also checks Lua syntax. CI installs Lua 5.1.

## Pull requests

Use focused commits. Explain user-visible behavior, privacy implications, schema changes, and verification performed. Schema changes must be versioned and accompanied by anonymized fixtures and tests. Do not commit generated desktop or addon packages.

By contributing, you agree that your contribution is licensed under the repository's MIT License.
