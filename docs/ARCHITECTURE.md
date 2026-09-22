# Architecture

Status: **accepted**, 22 September 2026. This document records the decision the
repository is built around. Earlier baselines explored several directions at
once; this one commits to a single shape.

## Context

WoW Coach is a local-first coaching companion for World of Warcraft
Anniversary / TBC Classic, built for other players rather than for its author.
That has two consequences that drive everything below:

1. **Windows and macOS both matter.** Classic players skew heavily Windows, so a
   Mac-only product addresses a minority of the audience.
2. **Distribution friction is the main risk**, not implementation difficulty.
   Every install step, signing warning, and platform-specific build pipeline
   costs users before they ever see a recommendation.

Three prototypes existed when this decision was made: a working SwiftUI macOS
app that reverse-engineered third-party addons' SavedVariables, a Rust core with
identity and snapshot models, and a Tauri 2 shell that could not yet import
anything.

## Decision

### 1. The collector addon is the contract

`addon/WoWCoachCollector` is the foundation of the product, not an accessory to
it. Everything the desktop clients need is captured in-game through the WoW API
and written to SavedVariables in a versioned schema this project controls.

The rejected alternative was to keep reverse-engineering other addons. The
SwiftUI prototype extracted bags, quests, professions, talents and recipes by
unpacking DataStore's bit-packed tables from the outside — decoding `BaseInfo`
bitfields, and aligning five independent Lua arrays *by position* on the
assumption that index N is the same character in each. That assumption fails
silently: it produces one character's bags attributed to another, with no error.

An addon we own asks the game directly and gets names, counts and ranks as
values. It is also identical on Windows and macOS, which the parsing of
third-party addon internals is not guaranteed to be.

### 2. One core, in Rust, compiled twice

`crates/wow-coach-core` owns the SavedVariables parser, the domain models, and
the coaching rule engine. It compiles natively for the desktop app and to
`wasm32` for the web client. The parsing and coaching logic is written and
tested once; the frontends are presentation only.

No client re-implements a rule. If a recommendation differs between web and
desktop, that is a bug in the build, not a difference in behaviour.

### 3. Two frontends, shared core

- `apps/web` — a browser client. The player drops their SavedVariables file onto
  the page; the WASM core parses it in the browser. No install, no code signing,
  no notarisation, and identical on both platforms. Where the File System Access
  API is available, the directory handle is retained so later visits re-read the
  file without re-picking it.
- `apps/desktop` — a Tauri 2 application. Discovers installations automatically,
  watches SavedVariables for changes, and keeps history in local SQLite.

### 4. Lua is parsed, never evaluated

The core never executes SavedVariables content. Because this project owns both
the writer (the addon) and the reader (the core), the addon deliberately emits a
restricted subset — table literals with string and number values only, no
functions, no references, no exotic keys — and the parser accepts only that
subset and rejects anything else.

### 5. The SwiftUI prototype is reference, not product

`reference/swift-v1.4.2` is archived, not maintained. Its value is the domain
knowledge encoded in it, which is ported into the core with tests: the DataStore
bit layouts, the TBC quest difficulty bands, the rested-XP accrual rate, and the
first pass at coaching heuristics. The application itself is not built, shipped,
or fixed.

## Data flow

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

Character data never leaves the player's machine in either client. There is no
telemetry, no account, and no upload path. Any future sharing must be a separate,
explicit user action.

## Repository layout

```text
addon/WoWCoachCollector/   the in-game collector — the data contract
crates/wow-coach-core/     parser, models, coaching rules (native + wasm)
apps/web/                  browser client
apps/desktop/              Tauri 2 desktop client
reference/swift-v1.4.2/    archived macOS prototype, kept as specification
fixtures/                  anonymized examples only
scripts/                   addon validation and packaging
docs/                      this decision record, privacy, disclaimer, roadmap
```

## Rejected alternatives

**Promote the SwiftUI app to the main line.** It works today and contains the
only proven parsing code, but it is macOS-only and Swift has no credible Windows
story. For a product aimed at other people, that excludes most of the audience.

**Start the repository again.** The existing scaffolding — licence, CI for both
Rust and the addon, packaging scripts, and the privacy and disclaimer posture —
would have been rebuilt almost identically. The problem was the framing, not the
contents.

**Desktop only.** Requires an Apple Developer membership, notarisation, and a
Windows signing certificate before the first stranger sees a recommendation;
without signing, users get an unrecognised-application warning on a tool that
reads their game files, which is fatal for trust.

**Web only.** Cannot discover installations or diff sessions in the background,
which is where the product gets genuinely useful.

## Known risks

- **Addon adoption.** The product is worth nothing to a player who will not
  install the collector. Mitigation is to make the collector small, fast, and
  independently useful; a fallback importer for DataStore/Altoholic users is
  deliberately deferred rather than ruled out.
- **Schema churn.** The addon's schema is a public contract the moment anyone
  installs it. It is versioned from the start and the parser must accept older
  versions.
- **Restricted-subset drift.** The writer and the parser must agree on the
  emitted subset. They are tested against shared fixtures for that reason.
