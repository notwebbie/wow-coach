# WoW Coach

WoW Coach is an early, local-first companion for **World of Warcraft Anniversary / TBC Classic**. The project is being rebuilt as a public Mac + Windows desktop app, a shared Rust core, and a CurseForge-ready in-game collector addon.

> This repository is a focused baseline, not a finished coaching product. It does not yet import the collector's SavedVariables into the desktop UI or generate recommendations.

## What exists today

- `wow-coach-core`: tested Rust models for account/flavor/realm/character identity, snapshots, history entries, and the legacy report schema-v3 import shape.
- `desktop`: a minimal Tauri 2 + React/TypeScript shell with the first SQLite history migration.
- `addon/WoWCoachCollector`: a local-only Lua collector with versioned SavedVariables and basic character fields.
- `legacy/macos-swift-v1.2`: the supplied macOS Swift prototype, retained for reference with personal defaults and fixtures replaced by generic examples.
- CI for Rust quality gates and addon validation/packaging.

## Repository layout

```text
addon/WoWCoachCollector/       WoW addon source
crates/wow-coach-core/         shared Rust models
desktop/                       Tauri 2 and React shell
fixtures/                      anonymized examples only
legacy/macos-swift-v1.2/       sanitized legacy prototype
scripts/                       addon validation and packaging
```

## Develop

Prerequisites: a current Rust toolchain, Node.js 20+, npm, and (for desktop system packages) the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cd desktop && npm ci && npm run build
```

The React shell and default Rust crate build without generated assets. Native Tauri launch is deliberately deferred because Tauri requires generated binary icon files, which this public baseline does not commit. Follow the exact local-only step in [`desktop/README.md`](desktop/README.md); generated icons remain ignored.

Validate and package the addon:

```sh
./scripts/validate-addon.sh
./scripts/package-addon.sh 0.1.0
unzip -l dist/WoWCoachCollector-0.1.0.zip
```

The generated ZIP contains one top-level `WoWCoachCollector` directory in CurseForge-compatible layout. Build output under `dist/` is intentionally ignored.

## Data flow and privacy

The addon writes only to WoW SavedVariables. It performs no network calls and no protected combat automation. The desktop direction is local file import into a local SQLite history database. No telemetry, cloud sync, account login, or upload path exists in this baseline.

Character snapshots are still personal data. Do not commit SavedVariables, reports, databases, or real character fixtures. Read [Privacy](docs/PRIVACY.md) before sharing diagnostic material.

## Scope and roadmap

Anniversary / TBC Classic is first. Retail and other game flavors may be modeled but are not a current product promise. See [Roadmap](docs/ROADMAP.md) for the deliberately staged plan.

## Disclaimer

WoW Coach is an independent community project and is not affiliated with or endorsed by Blizzard Entertainment. World of Warcraft and Blizzard Entertainment are trademarks or registered trademarks of Blizzard Entertainment, Inc. See the full [Disclaimer](docs/DISCLAIMER.md).

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md). The project is licensed under the existing [MIT License](LICENSE).
