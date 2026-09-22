# WoW Coach 1.4.1 for macOS

A local-first SwiftUI utility that discovers World of Warcraft installations and accounts, summarizes supported addon SavedVariables, tracks progress between reports, and creates a shareable ZIP on your Desktop.

Version 1.4.1 corrects quest title, level, and difficulty parsing when SavedVariables contain Unicode text, and normalizes duplicated recipes under their canonical profession. Version 1.4 added a configurable character rotation, measured versus estimated rested XP, class-trainer and unspent-talent reminders, quest-log pressure warnings, known profession recipe extraction, a broader profession-material inventory, TBC Wowhead quest URLs, and Auctionator market/full-scan metadata.

## Run

1. Install the full Xcode app from Apple.
2. Open `Package.swift` in Xcode.
3. Select the `WoWCoach` scheme and **My Mac**, then Run.

From Terminal, development builds also work with `swift run`.

## Build a standalone app

Run `zsh build-app.sh`. The signed application is created at `dist/WoW Coach.app`. Move it to `/Applications` and launch it like any other Mac app. No paid developer account is required for this local ad-hoc signed build.

Before creating a report, log out of WoW or run `/reload`. The app automatically checks `/Applications/World of Warcraft` and `~/Applications/World of Warcraft`; Settings can point it elsewhere.

## Architecture

- `WoWDiscovery` finds multiple game flavors and accounts.
- `SavedVariablesAnalyzer` is the extension point for independent addon analyzers.
- The DataStore analyzer supports both legacy keyed records and the compact array/bit-field records used by TBC Anniversary.
- `AnalyzerRegistry` composes plugins without coupling them to the UI or packager.
- `ReportBuilder` copies an allowlist of addon files, writes schema-versioned JSON, and uses macOS `ditto` to create a ZIP.
- `AppModel` and SwiftUI views manage selection, settings, and report status.

To add an analyzer, conform a new value type to `SavedVariablesAnalyzer` and register it in `AnalyzerRegistry.standard`.

## Privacy

Reports can expose character names, realms, inventory, auction history, and addon settings. Review the ZIP before uploading it. The app does not connect to the network.
