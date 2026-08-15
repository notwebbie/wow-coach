# Roadmap

This roadmap describes direction, not promised release dates.

## Baseline — complete in this repository

- Public monorepo boundaries and protective ignore rules.
- Sanitized archival copy of the macOS Swift 1.2 prototype.
- Collision-safe Rust identity plus snapshot/history models.
- Legacy schema-v3 JSON import shape and anonymized fixture.
- Tauri 2/React shell and SQLite snapshot-history migration.
- Versioned, local-only Anniversary/TBC Classic addon collector.
- Reproducible CurseForge-style ZIP packaging and CI checks.

## Next: local import slice

- Discover supported WoW installations on macOS and Windows.
- Parse `WoWCoachCollectorDB` without evaluating arbitrary Lua.
- Validate schema versions and surface actionable import errors.
- Persist snapshots transactionally to SQLite.
- Add fixture-based parser tests and migration tests.

## Then: useful history

- Character selector and snapshot timeline.
- Level, XP, currency, zone, and profession deltas.
- ECharts visualizations after the import/history flow is proven.
- Explicit local export with preview and redaction controls.

## Later: coaching and distribution

- Transparent, rule-based Anniversary/TBC Classic suggestions.
- Signed Mac and Windows releases.
- CurseForge metadata, changelog automation, and release checks.
- Broader game-flavor support only after the first path is reliable.

## Explicitly out of scope

- Botting, protected combat automation, or input broadcasting.
- Reading game process memory.
- Uploading character data without a separate, explicit user action.
- Claiming complete optimization or authoritative gameplay advice.
