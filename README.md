# WoW Coach

WoW Coach is a local-first coaching companion for **World of Warcraft
Anniversary / TBC Classic**. An in-game addon records what your characters are
doing; a shared Rust core turns that into progress history and plain-language
recommendations; web and desktop clients present it. Your character data never
leaves your machine.

> Early development. The collector addon and the shared models exist; the parser,
> coaching engine, and clients are being built in that order. Nothing here yet
> produces recommendations.

## How it fits together

```text
in-game addon  ──writes──▶  SavedVariables (versioned, project-owned schema)
                                   │
                            ┌──────┴───────┐
                    file drop│              │file watch
                             ▼              ▼
                     apps/web (WASM)   apps/desktop (Tauri)
                             └──────┬───────┘
                                    ▼
                          crates/wow-coach-core
                    parser · models · coaching rules
```

The addon is the contract. The Rust core is written once and compiled both
natively and to WebAssembly, so the web and desktop clients run identical parsing
and identical coaching rules. Neither client re-implements a rule.

[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) records why the project is shaped
this way, including what was rejected.
[`docs/COLLECTOR-SCHEMA.md`](docs/COLLECTOR-SCHEMA.md) is the addon's data
contract — the format every client reads.

## Addons

| Addon | | What it gives you |
|---|---|---|
| **WoWCoachCollector** | required | Your characters. Nothing works without it. It ships in this repository. |
| **Auctionator** | optional | Auction prices, and therefore anything about what is worth crafting or selling. |

Auctionator is the one outside addon this project leans on, and deliberately.
Everywhere else, collecting the data ourselves beats reading another addon's
saved state — that is the whole architecture. Auction prices are the exception:
our own addon can only see scans you personally run, while Auctionator holds a
price history built from every scan you have ever done. Without it the economy
features are simply absent; nothing else is affected.

`wow-coach doctor` reports which are installed, per game flavor, and says how to
get Auctionator if it is missing.

## Repository layout

```text
addon/WoWCoachCollector/   the in-game collector — the data contract
crates/wow-coach-core/     parser, models, coaching rules (native + wasm)
apps/web/                  browser client
apps/desktop/              Tauri 2 desktop client
reference/swift-v1.4.2/    archived macOS prototype, kept as specification
fixtures/                  anonymized examples only
scripts/                   addon validation and packaging
docs/                      architecture, privacy, disclaimer, roadmap
```

`reference/swift-v1.4.2` is a working SwiftUI macOS prototype that reached the
problem first. It is archived, not maintained: its value is the domain knowledge
being ported into the core — DataStore bit layouts, TBC quest difficulty bands,
rested-XP accrual, and a first pass at coaching heuristics.

## Develop

Prerequisites: a current Rust toolchain, Node.js 20+, npm, Lua 5.1 for the addon
tests, and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
for the desktop client.

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && npm ci && npm run build
```

Native Tauri launch needs generated binary icons, which are not committed. See
[`apps/desktop/README.md`](apps/desktop/README.md) for the local-only step.

Validate and package the addon:

```sh
./scripts/validate-addon.sh
./scripts/package-addon.sh 0.1.0
unzip -l dist/WoWCoachCollector-0.1.0.zip
```

The ZIP contains one top-level `WoWCoachCollector` directory in a
CurseForge-compatible layout. Output under `dist/` is ignored.

## Privacy

The addon writes only to SavedVariables. It makes no network calls and performs
no protected combat automation. Parsing happens on your own machine in both
clients — in the browser via WebAssembly, or locally in the desktop app. There is
no telemetry, cloud sync, account login, or upload path.

Character snapshots are still personal data. Do not commit SavedVariables,
reports, databases, or real character fixtures. Read
[Privacy](docs/PRIVACY.md) before sharing diagnostic material.

## Scope

Anniversary / TBC Classic is first. Other game flavors may be modeled but are not
a current promise. See [Roadmap](docs/ROADMAP.md).

Out of scope, permanently: botting, protected combat automation, input
broadcasting, reading game process memory, and uploading character data without a
separate explicit action by the user.

## Disclaimer

WoW Coach is an independent community project and is not affiliated with or
endorsed by Blizzard Entertainment. World of Warcraft and Blizzard Entertainment
are trademarks or registered trademarks of Blizzard Entertainment, Inc. See the
full [Disclaimer](docs/DISCLAIMER.md).

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md). Licensed
under the [MIT License](LICENSE).
