//! Levelling a profession, which is a different question from making money.
//!
//! [`crate::crafting`] answers "what is worth making", ranked by margin. This
//! module answers "what should I make to raise the skill", and the two orders
//! are close to opposite: the craft that raises a skill is usually the one
//! that loses a silver, because everybody makes it and nobody wants it. A
//! reader given only the profitable list would skip exactly the recipes that
//! advance them, and a reader given only this list would burn gold they meant
//! to keep. Keeping both, separately, is the point.
//!
//! # What the game already says, and what it does not
//!
//! The client classifies each recipe against current skill — optimal, medium,
//! easy, trivial — and the collector stores that verbatim. Those are the
//! orange, yellow, green and grey a player sees, and they are the whole
//! skill-up model: a trivial recipe never raises the skill, and the others do
//! with falling probability. That is the game's own judgement about its own
//! rules, and re-deriving it from skill numbers would be guessing at an answer
//! already given.
//!
//! What the game does not say is whether the materials are in reach, which
//! comes from the roster's holdings, or whether a trainer visit is due, which
//! is arithmetic on rank and character level.
//!
//! # Advice with a gate it cannot check
//!
//! Disenchanting is the worked example, and it is deliberately **not**
//! implemented. "Disenchant your greens" is standard advice for a levelling
//! enchanter and it is wrong for a specific one: disenchanting has an item
//! level gate, so a character at Enchanting 12 cannot touch their own level 35
//! drops. The engine could only check that gate if it knew the item level of
//! every green in the bags, and the collector captures item ids and counts —
//! not item levels.
//!
//! So the rule is not softened, hedged or shipped with a caveat. It is absent,
//! and this paragraph records why, so that the next person to notice the gap
//! finds a decision rather than an oversight.
//!
//! # Two limits found against live data, both real
//!
//! **Shortfalls are one step deep.** A tailor at 175 holding 34 Silk Cloth is
//! reported as short of three Bolts of Silk Cloth — which is true, and less
//! useful than it could be, because he knows the recipe that turns the cloth
//! into the bolts. The plan stops at "you are short of X" rather than asking
//! whether X is itself makeable. Going deeper means walking a dependency
//! graph and deciding how many intermediate steps count as "can make now",
//! and an answer that quietly assumed two crafting sessions were one would be
//! worse than the shallow one. Left shallow, and named.
//!
//! **Gathering professions cannot appear here at all.** Everything below is
//! keyed off the recipe cache, which is the only locale-independent evidence
//! that a skill is a trade skill (see `economy::SkillEntry::advice_is_safe`).
//! Skinning and Herbalism have no trade skill window and so no recipes, so a
//! skinner sitting at their cap gets no "train Expert" line even though the
//! arithmetic is the same. Mining escapes this only because Smelting gives it
//! a window. Fixing it means a second, trusted way to tell a gathering skill
//! from a language or a weapon proficiency, which is the exact problem the
//! recipe-cache rule was adopted to avoid guessing at.

use std::collections::{BTreeMap, BTreeSet};

use crate::collector::{ProfessionRecipes, Recipe};
use crate::economy::Holding;
use crate::roster::RosterEntry;

/// How the client rates a recipe against current skill.
///
/// The game's own words, not an interpretation of them. `Unknown` is for a
/// capture that predates the field or a client that words it differently, and
/// it is never silently treated as one of the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Difficulty {
    /// Orange. Always raises the skill.
    Optimal,
    /// Yellow. Usually raises it.
    Medium,
    /// Green. Sometimes.
    Easy,
    /// Grey. Never.
    Trivial,
    Unknown,
}

impl Difficulty {
    pub fn parse(text: Option<&str>) -> Self {
        match text {
            Some("optimal") => Self::Optimal,
            Some("medium") => Self::Medium,
            Some("easy") => Self::Easy,
            Some("trivial") => Self::Trivial,
            _ => Self::Unknown,
        }
    }

    /// Whether making this can still raise the skill.
    ///
    /// `Unknown` counts as "maybe": an uncaptured difficulty is missing data,
    /// and dropping the recipe would hide a real option, where listing it
    /// costs a reader one glance.
    pub fn can_raise_skill(&self) -> bool {
        !matches!(self, Self::Trivial)
    }

    pub fn colour(&self) -> &'static str {
        match self {
            Self::Optimal => "orange",
            Self::Medium => "yellow",
            Self::Easy => "green",
            Self::Trivial => "grey",
            Self::Unknown => "unrated",
        }
    }
}

/// One training tier: how far it lets a skill go, and what it asks first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tier {
    pub name: &'static str,
    /// The cap this tier raises the skill to.
    pub cap: u16,
    /// Character level required to train it.
    pub level: u16,
    /// Skill required before a trainer will teach it.
    pub skill: u16,
}

/// The training tiers, in order.
///
/// Two gates, not one, and they fail differently. Short on skill means keep
/// crafting; short on level means the profession is not the thing to work on
/// at all. Telling a player to visit a trainer they cannot buy from is the
/// failure this table exists to prevent.
///
/// **Master is not universal.** It arrived with The Burning Crusade. Vanilla
/// stops at Artisan 300, and so does WoW Forever — which is why this table is
/// never read directly, only through [`tiers_for`].
///
/// Sources for every value here, and the rule for adding another, are in
/// `docs/SOURCES.md`.
const TIERS: &[Tier] = &[
    Tier {
        name: "Apprentice",
        cap: 75,
        level: 5,
        skill: 0,
    },
    Tier {
        name: "Journeyman",
        cap: 150,
        level: 10,
        skill: 50,
    },
    Tier {
        name: "Expert",
        cap: 225,
        level: 20,
        skill: 125,
    },
    Tier {
        name: "Artisan",
        cap: 300,
        level: 35,
        skill: 200,
    },
    Tier {
        name: "Master",
        cap: 375,
        level: 55,
        skill: 275,
    },
];

/// The tiers that exist on one client.
///
/// Master (300 → 375) came in with The Burning Crusade. Vanilla ends at
/// Artisan 300, and **WoW Forever ends there too** despite being a modern
/// client — it is vanilla-era content, so its professions stop where vanilla's
/// did. Offering a Forever character a Master trainer would send them looking
/// for an NPC that does not exist, which is the precise failure the level gate
/// was built to prevent, arriving by a different door.
///
/// An unknown flavor keeps every tier. A client this build predates is far
/// likelier to be a later one than an earlier one, and the cost of being wrong
/// that way is a tier offered too early rather than a cap silently invented.
pub fn tiers_for(flavor: Option<&str>) -> &'static [Tier] {
    match flavor {
        Some("forever") | Some("classic_era") => &TIERS[..4],
        _ => TIERS,
    }
}

/// The tier that would raise this skill past `max_rank` on this client.
///
/// Matched on the cap rather than by counting upward, because a racial bonus
/// moves the cap without moving the tier: a Blood Elf enchanter sits at 85 on
/// an Apprentice's 75, and reading 85 as "some tier above Apprentice" would
/// skip a training step they still need.
pub fn next_tier(max_rank: u16, flavor: Option<&str>) -> Option<&'static Tier> {
    tiers_for(flavor).iter().find(|tier| tier.cap > max_rank)
}

/// Whether a trainer can help yet, and if not, what is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Training {
    /// Not near the cap; nothing to buy.
    NotYet,
    /// At the cap, and both gates are met. Go and train.
    Due { tier: &'static Tier },
    /// At the cap, high enough level, but the trainer wants more skill first.
    ///
    /// Only reachable with a racial bonus or a rounding oddity, and kept
    /// explicit rather than folded into `NotYet` so it cannot be mistaken for
    /// "nothing to do".
    ShortOnSkill { tier: &'static Tier, needs: u16 },
    /// At the cap and too low a level. The profession is capped until the
    /// character levels, which is advice about the character, not the skill.
    ShortOnLevel { tier: &'static Tier, needs: u16 },
    /// Nothing above this: the skill is finished for this client.
    Finished,
}

impl Training {
    pub fn is_actionable(&self) -> bool {
        matches!(self, Self::Due { .. })
    }
}

/// Assess training for one skill.
pub fn training(rank: u16, max_rank: u16, character_level: u16, flavor: Option<&str>) -> Training {
    if rank < max_rank {
        return Training::NotYet;
    }
    let Some(tier) = next_tier(max_rank, flavor) else {
        return Training::Finished;
    };
    if character_level < tier.level {
        return Training::ShortOnLevel {
            tier,
            needs: tier.level,
        };
    }
    if rank < tier.skill {
        return Training::ShortOnSkill {
            tier,
            needs: tier.skill,
        };
    }
    Training::Due { tier }
}

/// A recipe that can still raise the skill, and whether it can be made.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillUp {
    pub recipe: String,
    pub difficulty: Difficulty,
    /// How many the roster's materials allow, counting bags and banks.
    pub can_make: u32,
    /// Reagents that fall short, with what is missing.
    pub short_of: Vec<(String, u32)>,
    /// True when some of the materials are banked rather than to hand.
    pub needs_a_bank_trip: bool,
}

impl SkillUp {
    pub fn is_makeable(&self) -> bool {
        self.can_make > 0
    }
}

/// One profession on one character.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfessionPlan {
    pub character: String,
    pub profession: String,
    pub rank: Option<u16>,
    pub max_rank: Option<u16>,
    pub training: Training,
    /// Recipes that can still raise the skill, best first: makeable before
    /// not, then orange before yellow before green.
    pub skill_ups: Vec<SkillUp>,
    pub notes: Vec<String>,
}

impl ProfessionPlan {
    /// What to make next, if anything on hand still raises the skill.
    pub fn craft_next(&self) -> Option<&SkillUp> {
        self.skill_ups.iter().find(|entry| entry.is_makeable())
    }

    /// True when every recipe known here is grey.
    ///
    /// The state that reads as "nothing to do" and means the opposite: the
    /// player needs recipes, not materials, and no amount of crafting what
    /// they have will move the number.
    pub fn out_of_useful_recipes(&self) -> bool {
        self.skill_ups.is_empty()
    }
}

/// How many of `recipe` the roster's materials allow.
fn how_many(
    recipe: &Recipe,
    market: &Option<String>,
    held: &BTreeMap<(Option<String>, u32), &Holding>,
) -> (u32, Vec<(String, u32)>, bool) {
    let reagents = recipe.reagents.as_deref().unwrap_or(&[]);
    if reagents.is_empty() {
        // Not "free": uncaptured. Claiming a craft is possible on no evidence
        // is the error this guards.
        return (0, Vec::new(), false);
    }

    let mut possible = u32::MAX;
    let mut short = Vec::new();
    let mut banked = false;

    for reagent in reagents {
        let Some(item_id) = reagent.item_id else {
            continue;
        };
        let needed = reagent.count.unwrap_or(1).max(1);
        let holding = held.get(&(market.clone(), item_id));
        let have = holding.map(|holding| holding.count).unwrap_or(0);
        let in_bags: u32 = holding
            .map(|holding| holding.held_by.values().map(|amount| amount.bags).sum())
            .unwrap_or(0);
        if have >= needed && in_bags < needed {
            banked = true;
        }
        if have < needed {
            let name = reagent
                .name
                .clone()
                .or_else(|| holding.and_then(|holding| holding.name.clone()))
                .unwrap_or_else(|| format!("item {item_id}"));
            short.push((name, needed - have));
        }
        possible = possible.min(have / needed);
    }

    if possible == u32::MAX {
        possible = 0;
    }
    (possible, short, banked)
}

/// Build the profession plans for a roster.
///
/// Every character counts, whatever its role. A bank alt's professions are
/// most of the reason it exists, and a parked character's half-levelled
/// tailoring is still a fact about the account.
pub fn plan(entries: &[RosterEntry<'_>], holdings: &[Holding]) -> Vec<ProfessionPlan> {
    let mut held: BTreeMap<(Option<String>, u32), &Holding> = BTreeMap::new();
    for holding in holdings {
        held.insert((holding.market.clone(), holding.item_id), holding);
    }

    let mut plans = Vec::new();
    for entry in entries {
        let record = entry.record;
        let market =
            crate::auctionator::realm_key(record.realm.as_deref(), record.faction.as_deref());
        let level = record.level.unwrap_or(0);

        for (profession, cache) in record.recipes.iter().flatten() {
            plans.push(one(
                entry.name(),
                profession,
                cache,
                level,
                record.game_flavor.as_deref(),
                &market,
                &held,
            ));
        }
    }

    plans.sort_by(|a, b| {
        b.training
            .is_actionable()
            .cmp(&a.training.is_actionable())
            .then_with(|| a.character.cmp(&b.character))
            .then_with(|| a.profession.cmp(&b.profession))
    });
    plans
}

#[allow(clippy::too_many_arguments)]
fn one(
    character: &str,
    profession: &str,
    cache: &ProfessionRecipes,
    level: u16,
    flavor: Option<&str>,
    market: &Option<String>,
    held: &BTreeMap<(Option<String>, u32), &Holding>,
) -> ProfessionPlan {
    let mut notes = Vec::new();
    let recipes = cache.recipes.as_deref().unwrap_or(&[]);

    let training = match (cache.rank, cache.max_rank) {
        (Some(rank), Some(max)) => training(rank, max, level, flavor),
        _ => {
            notes.push("Rank was not captured, so nothing can be said about training.".to_string());
            Training::NotYet
        }
    };

    let mut without_reagents = 0usize;
    let mut skill_ups: Vec<SkillUp> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();

    for recipe in recipes {
        let difficulty = Difficulty::parse(recipe.difficulty.as_deref());
        if !difficulty.can_raise_skill() {
            continue;
        }
        let name = recipe.name.as_deref().unwrap_or("(unnamed)");
        if !seen.insert(name) {
            continue;
        }
        // `map_or(true, ..)` rather than `is_none_or`, which needs Rust 1.82
        // and this crate targets 1.77 so the wasm build stays buildable on
        // older toolchains.
        #[allow(clippy::unnecessary_map_or)]
        if recipe
            .reagents
            .as_ref()
            .map_or(true, |list| list.is_empty())
        {
            without_reagents += 1;
        }
        let (can_make, short_of, needs_a_bank_trip) = how_many(recipe, market, held);
        skill_ups.push(SkillUp {
            recipe: name.to_string(),
            difficulty,
            can_make,
            short_of,
            needs_a_bank_trip,
        });
    }

    skill_ups.sort_by(|a, b| {
        b.is_makeable()
            .cmp(&a.is_makeable())
            .then_with(|| a.difficulty.cmp(&b.difficulty))
            .then_with(|| a.recipe.cmp(&b.recipe))
    });

    if without_reagents > 0 {
        notes.push(format!(
            "{without_reagents} recipe(s) have no reagents captured, so they are listed as unmakeable rather than free. Open the profession window again."
        ));
    }
    if !recipes.is_empty() && skill_ups.is_empty() {
        notes.push(
            "Every known recipe here is grey. More crafting will not raise this skill — it needs new recipes from a trainer or a drop."
                .to_string(),
        );
    }

    ProfessionPlan {
        character: character.to_string(),
        profession: profession.to_string(),
        rank: cache.rank,
        max_rank: cache.max_rank,
        training,
        skill_ups,
        notes,
    }
}

/// Describe a training state in the words a player would use.
pub fn describe(training: &Training, profession: &str) -> Option<String> {
    match training {
        Training::NotYet => None,
        Training::Finished => None,
        Training::Due { tier } => Some(format!(
            "{profession} is at its cap — train {} ({}) to go to {}",
            tier.name, profession, tier.cap
        )),
        Training::ShortOnLevel { tier, needs } => Some(format!(
            "{profession} is capped until level {needs}, when {} can be trained",
            tier.name
        )),
        Training::ShortOnSkill { tier, needs } => Some(format!(
            "{profession} is at its cap but {} needs {needs} skill first",
            tier.name
        )),
    }
}
