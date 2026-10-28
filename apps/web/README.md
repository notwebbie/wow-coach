# Web client

Phase 4, and the player's interface. See [`../../docs/WEB-CLIENT-V1.md`](../../docs/WEB-CLIENT-V1.md)
for what version one does and why, and [`../../docs/INTERFACE.md`](../../docs/INTERFACE.md)
for why this is the interface and the CLI is not.

The player drops their `WoWCoachCollector.lua` onto the page and
[`crates/wow-coach-wasm`](../../crates/wow-coach-wasm), compiled to `wasm32`,
parses it in the browser. Nothing is uploaded. The same coaching rules run here
as in the desktop client, because it is the same compiled core.

## Running it

```sh
cargo install wasm-pack      # once
./build.sh
python3 -m http.server --directory . 8080
```

Then open <http://localhost:8080>. It has to be served over http: an ES module
will not load from a `file://` URL, and the page says so if you try.

## No bundler, no npm

One HTML file, one generated module, one shell script. The architecture rejected
the desktop-only route partly to avoid a build pipeline standing between the
project and its first user, and a browser client that needs a toolchain to host
would reintroduce that through a different door. `apps/desktop` carries npm
because Tauri needs it; this does not.

One consequence is worth naming, because it cost an evening. Without a bundler
the page loads the generated module itself, and a top-level `import` of it is
resolved *before* the page's own script runs. Forget to run `build.sh`, or
serve the folder without `pkg/`, and the browser discards the whole script:
no event listeners, no error, a page that looks loaded with every control
dead. So the engine is loaded with a dynamic `import()` at the foot of the
script, after the interface is wired up, and a missing build is reported on
screen with the command that fixes it. CI has a check that keeps it that way.

## What it shows

Three things, in the order a player wants them:

1. **Play next** — one character, with the sentence the core wrote. The rest
   of the ranking sits underneath.
2. **What to do with them** — the quest log and the professions for one
   character, picked with the selector and defaulting to whoever was just
   recommended. Quests give the hand-ins first, then the zone worth travelling
   to, then every zone with what it holds, and the log capacity with a warning
   when it is too full to accept anything. Professions give the rank, whether
   a trainer can help yet *and which gate is shut*, and what is still worth
   making with how many the roster's materials allow.
3. **Your characters** — the roster, and which of them are being levelled.

Two distinctions are deliberate and must survive any redesign. A profession
whose window has never been opened says so rather than reading as "nothing to
make" — absent is not empty, and this project has shipped that bug twice. And
a zone with nothing clearable is never offered as somewhere to go, even when
it is the only zone in the log.

Economy, crafting and travel are built in the core and not yet on the page.

## What is stored

Roles — which characters you are levelling — are kept in `localStorage`, so a
return visit does not ask again. That is per-browser and never leaves the
machine. Everything else is read from the file each time.

## Not in version one

**Remembering the file.** The roadmap calls for retaining the directory handle
so a return visit re-reads without re-picking, through the File System Access
API. That API is Chrome and Edge on desktop only — Firefox has never shipped it
and Safari still has not — so it is a genuine split rather than a fallback, and
it is worth building deliberately rather than bolting on. Until then every visit
starts with the file.

Economy, crafting, travel and history timelines all exist in the core and are
all still second screens. Quests and professions were the first two to come
across, because they are what a player needs the moment the recommendation has
told them who to log in as.
