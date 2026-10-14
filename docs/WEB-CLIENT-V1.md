# Web client, version one

Phase 4 of [`ARCHITECTURE.md`](ARCHITECTURE.md). Scope decided 14 October 2026.
See [`INTERFACE.md`](INTERFACE.md) for why this is the player's interface and
the CLI is not.

## Shape

**One build, two states.** On load the page asks whether it already knows this
player — are there saved roles, is there a file handle it may re-read. No, and
it runs the first-run sequence. Yes, and it goes straight to the dashboard. One
codebase, one compiled core, one branch at the top. There is no separate
"new user" build.

**Desktop only.** The SavedVariables file lives on the machine where WoW is
installed, so that is where the page is used. No mobile layout, no sync, no
accounts, nothing hosted. A player who wants to reach it from elsewhere can
tunnel to their own machine; that is outside this project.

This keeps the founding promise exact rather than approximate: character data
never leaves the player's machine, with no asterisk.

## The first run

1. **Drop the file**, with a line saying where to find it on Windows and on
   macOS — the step people actually get stuck on.
2. **Parse in the browser.** `crates/wow-coach-core` compiled to `wasm32`.
   Nothing is uploaded.
3. **The characters appear.** Names, levels, zone, what they hold. This is the
   moment the tool earns trust: not by teaching anything, but by being
   recognised. Get this screen right before anything else.
4. **"Which of these are you actually playing?"** Tick boxes. One question, not
   a classification exercise.
5. **One recommendation**, with its reasoning.

## Roles: ask, never guess

Ticked characters become `active`. **Unticked characters stay `Unclassified`**,
which is already the core's default and already means "not in the rotation".
Nothing is invented, and nobody is labelled something they did not say.

Guessing was considered and ruled out on evidence. The comment on
`classify::STORAGE_LEVEL` records that the heuristic was tested against a real
roster and lost — the two bank characters there are a level 20 warlock carrying
24 quests and a level 8 mage, neither proposable by any rule in the module. Its
conclusion stands:

> A played character and a bank character look the same from outside, because
> the player is the only one who knows which is which — so the answer is to
> make saying so easy (the roadmap's Phase 4), not to guess harder.

**The full role editor sits behind a button**, reachable at any time: all four
roles — active, bank, parked, reserved — per faction, every change reversible.
Nobody should have to understand the difference between parked and reserved in
their first thirty seconds.

**Deferred, and worth remembering:** unticked and `bank` are not the same thing
to the engine. The bank role decides whose bags and reagents the crafting view
counts. When crafting reaches the web client, players will need to mark their
banks properly. This costs nothing in version one, because crafting is out of
scope.

## The recommendation

**One answer, not a ranked list.** A single answer is what a coach gives: it is
unambiguous, hard to ignore, and when it is wrong it is obviously wrong, which
is the feedback worth having early. A list of five characters with reasons is a
thing people skim and close.

The rest of the ranking is available underneath for anyone who wants it.

Every recommendation states its reasoning, as Phase 3 requires.

## The browser split, accepted deliberately

Remembering the file between visits needs the File System Access API, which is
**Chrome and Edge on desktop only**. Firefox has never shipped it. Safari has
never shipped it and still has not, as of 27.1 and the current Technology
Preview. Chrome for Android does not support it either, which independently
confirms the no-mobile decision.

- **Chrome and Edge** — the handle is retained, return visits re-read without
  re-picking, and the dashboard state works as described.
- **Safari and Firefox** — every visit starts with a drop.

This split is accepted rather than engineered around. The obligation it creates
is that the fallback must be a **deliberate path, not a broken one**: detect the
browser, say so once and plainly, and never silently behave differently. That
sentence is the whole difference between a limitation and a bug.

Checked against [caniuse](https://caniuse.com/native-filesystem-api) on
14 October 2026. Re-check before relying on it; this is the kind of fact that
changes.

## Out of scope for version one

Economy and crafting valuation, the quest router, travel, profession levelling
plans, history timelines. All of them exist in the core already. All of them are
second screens.

Shipping depth before anyone has successfully dropped a file and read one
correct recommendation is building on an untested first minute.

## Success test

A player who is not the author drops their file and gets a correct
recommendation without reading any documentation.

Not "the features work". That one sentence.
