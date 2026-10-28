# Where the game knowledge comes from

This tool makes claims about World of Warcraft. Some are read from the player's
own client; some are constants written into the code. This document says which
is which, where each constant came from, and what this project will not do.

It exists because the distinction went unrecorded once and produced a wrong
answer. A profession tier table taken from a Burning Crusade source was applied
to every client, so a WoW Forever character at the Artisan cap would have been
told to train Master — a rank that does not exist on that client. Nothing in
the output said the table had a provenance, so nothing invited the question.

## Three kinds of claim, ranked by how much they can be trusted

**1. What the client reported.** Rank and cap, recipe difficulty, reagents, bag
and bank contents, known spells, quest state. This is the game's own account of
itself, it is locale-independent where ids are used, and it cannot go stale
against a patch because it is re-read every capture.

Prefer it always. When the client already answers a question — "is this recipe
still orange?" — re-deriving the answer from a guide is guessing at something
already known, and it rots.

A worked example of why, and of the one care this category needs. The question
"is this skill a profession or a language?" was answered for a year by the
recipe cache, which is evidence but partial: a skill with no trade skill window
never appears in it, so a capped skinner got silence. `GetProfessions` answers
it directly, and the collector now reads it. Its documented signature —
`prof1, prof2, archaeology, fishing, cooking, firstAid`
([wowprogramming](https://wowprogramming.com/docs/api/GetProfessions.html)) —
does **not** match what the Forever beta returned on build 70235: `6, 7, 5,
nil, 8, nil`, where index 5 is First Aid, sitting in the slot the documentation
calls archaeology. Rather than pick a winner, the collector reads no meaning
from position at all and asks `GetProfessionInfo` for each index, which names
the profession itself. A client-reported fact is still worth checking against
its documentation; the answer here was to depend on less of it.

**And a client-reported fact can still be a lie.** On the Anniversary client
`GetProfessions` exists and returns nothing, so the capability check passes and
the answer is empty — for characters that demonstrably have professions. The
first category in this document is "what the client reported", and it is still
the most trustworthy, but *reported* is not the same as *correct*. The fix was
to record how many values the call returned, so an empty answer and a broken
one stop being the same bytes. The gathering-profession gap that
`GetProfessions` closed on WoW Forever therefore remains open on Anniversary.

**2. A constant in the code, with a source.** Facts the client does not report:
what a teleport spell id means, where a training tier stops, how fast rested XP
accrues. Every one of these is listed below with where it came from.

The rules for adding one:

- Check it against a primary or near-primary source and **cite it in the code**,
  not only in a commit message.
- Prefer two independent sources when the value drives advice.
- Say which clients it applies to. Most of these are not universal, and the
  failure above came from assuming one was.
- Where the roster can confirm it, say so — a constant that matches live
  captured data has been checked a third way.

**3. Something Claude knew.** Unsourced recall. It is often right and it is not
evidence. It belongs in conversation, clearly marked, and it does not go into
the code or into a planning document without being checked first.

## The constants, and where each came from

| Constant | What it encodes | Source |
|---|---|---|
| `professions::TIERS` | Apprentice/Journeyman/Expert/Artisan/Master: cap, character level, skill required | [warcraft.wiki.gg — Classic proficiency](https://warcraft.wiki.gg/wiki/Classic_proficiency) and [Icy Veins — TBC Mining](https://www.icy-veins.com/tbc-classic/mining-profession-and-leveling-1-375-guide). Master is TBC-only; Forever and Classic Era stop at Artisan 300, per [Icy Veins — Forever professions](https://www.icy-veins.com/wow-forever/professions-overview) and [wow-professions — Forever](https://www.wow-professions.com/forever). Cross-checked against the live roster, where ranks sit exactly on 75/150/225. |
| `travel::TRAVEL_SPELLS` | 17 teleport and portal spell ids to destination, side and kind | Each id checked individually on its own Wowhead TBC spell page. A wrong id here does not fail loudly, it reports the wrong destination. |
| `rested::forever()` | Measured rested rate on WoW Forever, and an explicitly unknown cap | This project's own probe, 26 hours parked. The module carries the sample count and the unresolved alternative. |
| `collector::WELL_RESTED_SPELL` / `WELL_RESTED_NODE` | The Legacy perk that changes rested accrual | Spell `1225478`, node `110301` in the Adventure tree (1188), read from a live WoW Forever capture on 19 October 2026 where the client's own spell name resolves to "Well Rested", 5 ranks. Matched on the spell id, never the name — the same capture carries six nodes the client itself names "Unknown", and the names are English. |
| `rested::classic()` | 5% per 8 hours, cap 1.5 levels | Long-settled Classic behaviour, corroborated by a capped character on the live roster reading exactly 31,200 at level 20. |
| `economy::OUTLIER_RATIO` | 100x, before a last-seen price is called suspect | Calibrated against this player's own Auctionator data, not from a guide: scan-completeness artifacts reach ~30x and a genuine joke listing reached ~5,600x. The reasoning is in the code. |
| `quests::TOO_HIGH_BY` / `TRIVIAL_BY` / `TIGHT_WITHIN` | Quest-log routing thresholds | Judgement calls, not game facts. Labelled as such in the module. |
| `classify::STORAGE_LEVEL` / `IDLE_SECONDS` | Role inference heuristics | Judgement calls, and recorded as having been tested against a real roster and **failed** — see the comment on `STORAGE_LEVEL`. |

## Reference sites, by edition

Used for research and for hand-written planning. **None of their content is
bundled** — see the next section.

**All editions**

- [Wowhead](https://www.wowhead.com/) — the primary reference for spell and item
  ids. Per-expansion paths: `/classic/`, `/tbc/`. Use this to check an id rather
  than a guide's prose.
- [warcraft.wiki.gg](https://warcraft.wiki.gg/) — mechanics and long-settled
  systems.
- [Icy Veins](https://www.icy-veins.com/) — per-edition class, profession and
  quest guides.

**Burning Crusade / Anniversary** (`tbc_classic`)

- [wow-professions — TBC index](https://www.wow-professions.com/tbc), with a
  guide per profession, e.g.
  [Tailoring](https://www.wow-professions.com/tbc/tailoring-leveling-guide-burning-crusade-classic),
  [Enchanting](https://www.wow-professions.com/tbc/enchanting-leveling-guide-burning-crusade-classic),
  [Alchemy](https://www.wow-professions.com/tbc/alchemy-leveling-guide-burning-crusade-classic),
  [First Aid](https://www.wow-professions.com/tbc/first-aid-leveling-guide-burning-crusade-classic),
  [Mining](https://www.wow-professions.com/tbc/mining-leveling-guide-tbc-classic),
  [Skinning](https://www.wow-professions.com/tbc/skinning-leveling-guide-tbc-classic).
  Guides are shaped skill-bracket → recipe → quantity → materials, which is
  exactly the levelling path the tool cannot derive.
- [wow-professions — TBC disenchanting tables](https://www.wow-professions.com/tbc/disenchanting-tables)
- [Icy Veins — TBC profession guides](https://www.icy-veins.com/tbc-classic/profession-guides)

**WoW Forever** (`forever`)

- [wow-professions — Forever index](https://www.wow-professions.com/forever):
  [Alchemy](https://www.wow-professions.com/forever/alchemy-leveling-guide),
  [Blacksmithing](https://www.wow-professions.com/forever/blacksmithing-leveling-guide),
  [Enchanting](https://www.wow-professions.com/forever/enchanting-leveling-guide),
  [Engineering](https://www.wow-professions.com/forever/engineering-leveling-guide),
  [Leatherworking](https://www.wow-professions.com/forever/leatherworking-leveling-guide),
  [Tailoring](https://www.wow-professions.com/forever/tailoring-leveling-guide),
  [First Aid](https://www.wow-professions.com/forever/first-aid-leveling-guide),
  [Cooking](https://www.wow-professions.com/forever/cooking-leveling-guide),
  [Fishing](https://www.wow-professions.com/forever/fishing-leveling-guide).
- [wow-professions — Camping](https://www.wow-professions.com/forever/camping-guide).
  Camping is new in Forever and every profession feeds it, which the collector
  does not capture at all yet.
- [Icy Veins — Forever professions overview](https://www.icy-veins.com/wow-forever/professions-overview)

Forever is a live beta. Treat every Forever source as provisional and prefer
what the client reports, which is the whole reason the probe exists.

**Classic Era** (`classic_era`)

- [wow-professions — Classic index](https://www.wow-professions.com/classic)

## What this project will not do

**It will not bundle a levelling path.** Skill-bracket tables are the work of
the sites above and are theirs to publish. Copying them in would also freeze a
moving target into a binary that has no way to notice it has gone stale, which
is the failure mode this project designs against everywhere else.

There is a real gap underneath that refusal, and it is worth naming rather than
pretending away: `professions` can only advise on recipes the player already
knows, because the recipe cache is what it reads. It can say "make the Linen
Belt you know"; it cannot say "go and train Bolt of Mageweave next". A
levelling path is precisely what would close that.

A neighbouring gap has since closed, and the way it closed is the pattern to
copy. "A profession with no trade skill window is invisible to this tool" was
written down twice as a limitation — once wrongly, as a property of gathering
professions, and once correctly but still as something to live with. It was
fixed by finding a second thing the client already reports, not by adding a
constant. When a limitation here looks structural, the first question is
whether the game is willing to say it.

If it is closed, the shape is a path file the **player supplies** and the tool
reads — the same relationship as the roster config. The tool ships none, so
nothing is redistributed and nothing goes stale in a release.

**Camping is a Forever system the collector does not model.** Every profession
there gained campsite structures, which arrive as ordinary recipes — a live
capture shows Herbalism holding Seed Hybridizer, Greenhouse and Incense Candle,
and Alchemy carrying 197 recipes at skill 23 against 8 on Burning Crusade. The
tool reads them as recipes like any other, which is correct as far as it goes
and says nothing about campsites, buffs or what a structure does.

**A constant can be too broad as well as wrong.** The first version of the
rested-baseline check summed Legacy ranks across all three trees, on the
reasoning that any perk might matter. The first live capture disproved it
inside a minute: the character had one point in *Working Overtime*, a
professions perk, and would have been told his rested figures were approximate
when they were exact. The fix was to name the one perk that moves those
numbers. A constant that covers more than it should produces wrong answers just
as surely as one that covers too little, and it is harder to notice, because
the output still looks cautious.

**It will not present unsourced recall as fact.** A planning document that
mixes checked and unchecked claims without marking which is which is worse than
one that checks nothing, because it lends the unchecked parts the credibility
of the checked ones. This happened: a session plan cited four sources at the
bottom while several numeric claims in the body had none. Two of those were
later verified and held; the rest are still marked unverified.

## Quest database links

The quest screen links each quest to Wowhead by the client's own quest id. The
path prefix depends on which game the character is in, and the three prefixes
in `apps/web/index.html` are not equally well established:

| Flavor | Prefix | Standing |
|---|---|---|
| `forever` | `/forever/quest=<id>` | **Confirmed.** A live Wowhead URL in that form was returned by search on 19 October 2026. |
| `classic_era` | `/classic/quest=<id>` | **Not verified here.** The long-standing Classic prefix, not checked against a live page. |
| `tbc_classic` | `/tbc/quest=<id>` | **Not verified here.** Same, and the weaker of the two — Wowhead's Classic branches were renamed as Classic progressed through its expansions. |
| anything else | `/quest=<id>` | The bare form, used as a deliberate fallback. |

An unknown flavor falls through to the bare form rather than guessing a path,
because a link that 404s is worse than one that lands on the wrong edition: the
first looks like the tool is broken, the second is recoverable by the reader.

The two unverified rows are written down rather than quietly shipped because
the only client anyone has run this against is Forever, and a reader who
assumes all three were checked would be wrong. Checking them costs one page
load each by someone with a Classic or TBC character to test against.

## Gear milestones

`crates/wow-coach-core/src/gear.rs` carries the first curated dataset in this
project. Everything else in the core is measured from the client or derived by
rule from those measurements; a milestone is somebody's judgement that a wand
matters at level five. That difference is why each row stores its own
provenance in the data rather than in a comment, and why a test fails the build
if a row has no source or no date.

**The table is a seed, not a finished list.** It holds what was well sourced on
28 October 2026, a week before launch:

| Milestone | Classes | Source | Checked for Forever |
|---|---|---|---|
| A wand, from level 5 | Mage, Priest, Warlock | Warcraft Tavern, Forever warlock levelling guide | Yes |
| A better wand, from 13 | Mage, Priest, Warlock | Same | Yes |
| A ranged weapon | Hunter | Carried over from Classic | **No** |

The hunter row is marked unchecked in the data itself and that flag travels all
the way to the screen, because a reader cannot weigh a claim whose standing was
dropped on the way.

Six of the nine classes have no row at all. A class with nothing recorded is
reported as *silence*, with a caveat saying so — not as a clean bill of health.
This is the same absent-is-not-empty trap that has caught this project over bag
contents, profession lists and recipe caches, and it would have been easiest to
fall into here, where an empty list of advice looks exactly like good news.

**Why the table is small on purpose.** Full per-class levelling gear lists run
to hundreds of items, Forever's data will not settle for weeks after launch,
and stale gear advice is worse than none: a player who follows it wastes a
trip. The bar for a row is that *not having the item is a mistake*, not that
something better exists.

**A check can be too broad here too.** The first version of the
unidentified-item rule said that any worn item the client had not described
made every milestone unanswerable. The committed TBC fixture — a level 34 mage
wearing two uncached trinkets — then reported that it could not tell whether a
wand was equipped, with the ranged slot plainly empty. Whatever those trinkets
are, they are not wands; the client put them in trinket slots. The rule now
only blocks on an unidentified item in a slot that could hold the thing being
asked about. This is the third time a constant or check in this project has
been wrong by covering too much rather than too little.

## Craftable milestones and the skill gate

The wand rungs carry the Enchanting skill each needs, so the tool can say "ten
points short" rather than "cannot make this".

| Item | Enchanting skill | Source | Read |
|---|---|---|---|
| Lesser Magic Wand | 15 | Wowpedia | 2026-10-28 |
| Greater Magic Wand | 70 | AHledger, WoW Forever recipe page | 2026-10-28 |

Greater Magic Wand's **item level 23** is also load-bearing: it is the bar a
wand must clear to count as the upgrade rather than the thing being upgraded
from. Without a number there, "a better wand" was satisfied by the starter wand
the rung exists to replace — both are `Wands` and nothing else told them apart.
Any other wand at item level 23 or above satisfies it equally; the number is a
threshold, not a shopping list.

The Wowpedia figure for the Lesser Magic Wand is Classic data rather than a
Forever source, and the skill requirement is assumed unchanged. The AHledger
page is Forever-specific.

**Recipes are matched by name**, because the collector captures a recipe's name
and not what it produces. That makes the match locale-dependent: an English
client says "Greater Magic Wand" and a German one does not. A miss therefore
reports as "nobody can make it" rather than being silently dropped, which is
the safe direction, but it is a real limitation and the fix is the collector
capturing a recipe's output item id.
