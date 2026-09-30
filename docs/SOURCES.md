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

If it is closed, the shape is a path file the **player supplies** and the tool
reads — the same relationship as the roster config. The tool ships none, so
nothing is redistributed and nothing goes stale in a release.

**It will not present unsourced recall as fact.** A planning document that
mixes checked and unchecked claims without marking which is which is worse than
one that checks nothing, because it lends the unchecked parts the credibility
of the checked ones. This happened: a session plan cited four sources at the
bottom while several numeric claims in the body had none. Two of those were
later verified and held; the rest are still marked unverified.
