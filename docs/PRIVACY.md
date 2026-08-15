# Privacy

WoW Coach is designed as a local-first project.

## Baseline behavior

- The addon stores snapshots in `WoWCoachCollectorDB` through WoW's SavedVariables system.
- The addon has no network transport and does not automate protected gameplay actions.
- The desktop shell initializes a local SQLite database; collector import is not yet connected.
- There is no telemetry, cloud service, login, advertising SDK, or automatic upload.

## Data that may be sensitive

SavedVariables and future history records can contain character names, realms, account-folder labels, progression, location, currency, and other gameplay details. File-system paths can also reveal local usernames or account labels.

## Safe sharing

Never attach an entire `WTF` folder, SavedVariables file, generated report, or database to a public issue. Reproduce problems with the anonymized fixture format in `fixtures/`. Remove local paths and replace account, realm, and character labels with generic examples.

## Deletion

This baseline does not yet expose database controls in the UI. Development databases can be removed through the operating system's application-data directory. A documented in-app delete/export flow is required before a production release.

Privacy or data-handling concerns can be reported through the private security process in `SECURITY.md`.
