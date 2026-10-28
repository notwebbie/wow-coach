# Page tests

Drives `../index.html` in headless Chromium.

```sh
npm install
npx playwright install chromium
node page.mjs
```

The middle line is not optional on a fresh machine and is easy to miss: `npm
install` fetches Playwright's library, not the browser it drives. Without it
the run fails at `browserType.launch` with "Executable doesn't exist", which
reads like a broken harness rather than a missing download. It only needs
running once per machine, and again when Playwright's version changes.

A sandbox that already ships a Chromium can skip the download by pointing at
it: `CHROMIUM_PATH=/path/to/chromium node page.mjs`.

**If the download finishes and then hangs**, it is stalling on extraction, not
on the network — re-running just deletes the half-unpacked copy and fetches the
identical archive again. Endpoint security inspecting a freshly written browser
binary is the usual cause. The harness falls back to an installed Google
Chrome on its own in that case, so `node page.mjs` works without finishing the
download at all; `npx playwright install chromium` is the preferred path, not a
required one.

## Why this exists when the web client has no build step

`apps/web/README.md` says no bundler and no npm, and means it: the page is
served as it sits, and hosting it needs nothing but a static server. That is
about **hosting**, not testing, and this directory does not change it. Nothing
here is served, imported by the page, or needed to run it.

It earns its place because it has already caught two bugs that every other
check in this repo passed:

**A static import of the engine killed the page.** `import init from
"./pkg/..."` is resolved before the module body runs, so a missing build
discarded the whole script — no listeners, no error, a page that looked loaded
with a dead file picker. Rust tests cannot see it. The CI grep cannot see it.
Opening the page in a browser sees it immediately.

**`notices` was a `#[wasm_bindgen(getter)]` and the page called
`session.notices()`.** JavaScript saw a property, the call threw, and the
screen went blank the moment a file was dropped. Both sides spell it
`notices`; only the attribute decides what JavaScript gets, so no text search
could have found it.

That is the class of bug this catches: the seam between Rust and the page,
where both halves are individually correct.

## The stub

`stub/wow_coach_wasm.js` is a plain-JS stand-in for the `wasm-pack` output,
served at `/pkg/` during the run. The page is the thing under test, not the
engine — the engine has its own tests in Rust, and building wasm here would
make this slow enough that nobody would run it.

The stub's data is deliberately awkward where the page is most likely to get
it wrong: a profession whose recipe window has never been opened, one whose
recipes are all grey, a reagent that falls short, a zone holding only quests
that are out of reach, and a full quest log. **If the shape of a DTO in
`crates/wow-coach-wasm` changes, change it here too** — a stub that has
drifted from the real bindings tests nothing. The Rust side has its own
fixture-backed tests for the shapes; this side tests what the page does with
them.
