# Web client

Phase 4. The browser client for WoW Coach: the player drops their
`WoWCoachCollector.lua` SavedVariables file onto the page and
`crates/wow-coach-core`, compiled to `wasm32`, parses it in the browser.

Nothing is uploaded. The file is read locally and the same coaching rules run
here as in the desktop client, because it is the same compiled core.

Where the File System Access API is available, the chosen directory handle is
retained so a return visit re-reads the file without re-picking it. Browsers
without it fall back to drag-and-drop.

This directory is a placeholder until Phase 2 produces a core worth calling.
See [`docs/ROADMAP.md`](../../docs/ROADMAP.md).
