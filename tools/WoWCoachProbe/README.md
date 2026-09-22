# WoW Coach Probe — beta diagnostic

A throwaway addon for the WoW Forever beta. It is **not shipped** and not part
of the product. Its only job is to answer the questions about the Forever client
that reading Blizzard's UI source could not settle, before the collector is
written against assumptions.

The beta runs until **21 October 2026**. After that these answers cost a wait
until launch.

## Why this exists

Prior research established most of the Forever API from Blizzard's own shipped
Lua. Ten things could not be settled from source, and one of them could
invalidate the whole design:

**Secret Values.** Forever runs retail's Secret Values system. `QuestInfo.title`
is typed `string` rather than `cstring`, which may mean it cannot be read,
compared, concatenated, or written to SavedVariables. If quest titles are
secret, the quest capture path does not work as designed on this client and has
to store IDs only. Nothing short of trying it will tell us.

## Install

1. Copy the `WoWCoachProbe` folder into your Forever install:
   `<WoW>/_forever_/Interface/AddOns/WoWCoachProbe`
   The folder must contain `WoWCoachProbe.toc` and `WoWCoachProbe.lua` directly.
2. At the character select screen, click **AddOns** and make sure it is enabled.
   Tick **Load out of date AddOns** if the beta has moved past interface 16001.
3. Log in. It announces itself in chat and starts sampling rested XP on its own.

It writes only to its own SavedVariables. It makes no network calls and changes
nothing in game.

## The checklist

Do these in order. Each one answers a specific question, and the whole thing is
maybe twenty minutes plus some waiting.

### 1. First run — baseline and Secret Values

- [ ] Pick up at least one quest, so there is a quest in the log to inspect.
- [ ] Run `/wcprobe`
- [ ] Type `/reload`
- [ ] Run `/wcprobe` again

The second run is the one that matters: it reads back what the first run wrote
and reports whether the quest title survived. Anything reported in orange is a
probe that returned nothing or errored — that is a result, not a failure.

### 2. Ruleset

- [ ] Note which ruleset this character is on: ______________
- [ ] Run `/wcprobe` on a character on a **different** ruleset if you have one

This is the only way to confirm `PvPRuleset` and `RPRuleset` work. Blizzard's own
UI never calls them, so they are inferred from the enum alone. Hardcore is
already confirmed; Normal vs PvP vs Roleplaying is not.

### 3. Professions and skills

- [ ] Run `/wcprobe` **before** opening the Skills panel this session
- [ ] Open the Skills panel, then run `/wcprobe` again

If the skill count differs between the two, the collector cannot snapshot at
login and must wait for the panel or an event. Worth knowing before it silently
returns nothing for every player who never opens that panel.

### 4. Recipes — the important delta

- [ ] With every profession window **closed**, run `/wcprobe recipes`
- [ ] Open a profession, run `/wcprobe recipes`
- [ ] Close it again, run `/wcprobe recipes`

Three snapshots. If the closed ones return nothing, the window constraint from
the Classic client still applies and recipes stay an opportunistic cache. If
they return data, the collector can refresh recipes whenever it likes, which is
a meaningfully better product.

### 5. Legacy System

- [ ] Run `/wcprobe legacy` **before** ever opening the Legacy UI
- [ ] Open the Legacy UI, close it, run `/wcprobe legacy` again

This dumps every node in the Adventure tree by name, which is how we find the
**Well Rested** node ID. That perk changes rested accrual and the rested cap —
the numbers the coaching engine depends on — and nothing in the API exposes the
rate directly.

### 6. Rested XP — needs elapsed time, not effort

- [ ] Log out (or park) in an inn or city for a few hours, then log in
- [ ] Do this once with **Well Rested purchased** and once without, if you can

Sampling is automatic; you do not need to run anything. The addon logs
exhaustion against wall-clock time on every change, and the accrual rate gets
fitted from that offline. This is the only way to get the real number.

### 7. Camping

- [ ] Stand at a campsite and run `/wcprobe auras`
- [ ] Walk well away from it and run `/wcprobe auras` again

Camping has no API at all — no namespace, no events. The buff it applies is the
only handle, so the two snapshots get diffed to find its spell ID.

## Sending the results back

The file is at:

```text
<WoW>/_forever_/WTF/Account/<YOUR ACCOUNT>/SavedVariables/WoWCoachProbe.lua
```

Send that file. `/wcprobe report` prints how many runs and samples it holds if
you want to check before sending.

It contains your character name, realm/ruleset, level, zone, quest titles and
profession ranks. Nothing else, and nothing leaves your machine unless you send
it.

## Development

The two files under `tests/` run the probe against stubbed clients — one where
almost no API exists, one populated like Forever — and assert it neither errors
nor silently reports success for an absent API. Run them before changing
anything, because a probe that errors in game wastes a beta session:

```sh
cd tools/WoWCoachProbe
lua5.1 tests/probe_bare_client_test.lua
lua5.1 tests/probe_forever_client_test.lua
```
