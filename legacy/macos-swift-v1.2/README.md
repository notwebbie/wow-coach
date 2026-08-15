# WoW Coach 1.2 for macOS (legacy, sanitized)

This directory preserves the supplied SwiftUI prototype as historical implementation reference. Personal character defaults, objectives, dates, realm/account-style fixtures, and local data were replaced with generic examples before publication. No generated reports or raw SavedVariables are included.

The prototype discovers World of Warcraft installations and accounts, summarizes selected addon SavedVariables, tracks progress between reports, and creates a local ZIP. It is **not** the current cross-platform architecture and receives no feature development.

## Run the archived prototype

1. Install the full Xcode app.
2. Open `Package.swift` in Xcode.
3. Select the `WoWCoach` scheme and **My Mac**, then Run.

Development builds also work with `swift test` and `swift run` from this directory. `zsh build-app.sh` creates an ad-hoc local app under ignored `dist/` output.

## Privacy warning

The archived report workflow can copy supported live addon SavedVariables into a ZIP. Those files may contain character names, realms, inventory, auction history, mail, and addon settings. Review any archive before sharing and never commit it to this repository.
