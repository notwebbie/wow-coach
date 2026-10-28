//! Who on the roster supplies what, and by when.
//!
//! The question this answers is the one a player with alts actually has:
//! *my mage hits 13 next session and wants a better wand — can anybody I own
//! make one, and what has to happen first?*
//!
//! Three things are joined here and none of them is invented:
//!
//! - [`crate::gear`] says what a character is missing and from what level.
//! - [`crate::crafting`] says what each character can make right now, with the
//!   roster's materials, in their own market.
//! - The roster's profession ranks say who is *close* to being able to make
//!   something they cannot make yet.
//!
//! **The faction line is the hard constraint.** Alliance and Horde share no
//! mail, no auction house and no trade, so a Horde enchanter is no use to an
//! Alliance mage however good their skill. That boundary already exists in the
//! economy layer as the market key — realm plus faction — and this module
//! reuses it rather than re-deriving it. Where a character across the line
//! *could* have helped, that is reported as its own answer rather than as
//! silence, because "nobody can make this" and "your other faction can make
//! this and cannot send it" lead a player to different decisions.

use std::collections::BTreeMap;

use crate::auctionator;
use crate::auctionator::PriceDb;
use crate::collector::CharacterRecord;
use crate::crafting;
use crate::economy::Holding;
use crate::gear::{self, CraftRoute, Standing};
use crate::roster::RosterEntry;

/// How far ahead to look for milestones not yet due.
///
/// Levelling is the point: a wand wanted at 13 is worth starting at 10, and
/// one wanted at 40 is not worth thinking about at 8. Five levels is roughly a
/// session or two, which is the horizon the advice is written for.
pub const DEFAULT_LOOKAHEAD: u16 = 5;

/// What to do about one character's missing item.
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    /// This character can make it themselves, now.
    Themselves { recipe: String, can_make_now: u32 },
    /// Another character on the same realm and faction can make it now.
    RosterCrafter {
        crafter: String,
        recipe: String,
        can_make_now: u32,
    },
    /// A crafter on the right side has the profession and the recipe but not
    /// the materials.
    ShortOfMaterials {
        crafter: String,
        recipe: String,
        missing: Vec<Missing>,
    },
    /// A crafter on the right side has the profession but not enough skill.
    /// The gap is the actionable part.
    ShortOfSkill {
        crafter: String,
        profession: String,
        recipe: String,
        have: u16,
        need: u16,
    },
    /// Skill enough, but the recipe has not been learned.
    NotLearned {
        crafter: String,
        profession: String,
        recipe: String,
    },
    /// Somebody on the right side has the profession and enough skill, but
    /// nobody has opened their trade window, so whether they know the recipe
    /// is genuinely unknown.
    ///
    /// Ranked above the across-the-line answer on purpose. A same-faction
    /// crafter who *might* already be able to do this is a better lead than a
    /// cross-faction one who definitely can and definitely cannot send it, and
    /// an earlier version fell straight past the first to report the second.
    MaybeCapable {
        crafter: String,
        profession: String,
        rank: u16,
    },
    /// Someone across the faction line could make it, and nothing crosses.
    /// Named rather than hidden: it is the difference between "buy one" and
    /// "you already own the answer and cannot reach it".
    WrongFaction {
        crafter: String,
        faction: String,
        recipe: String,
    },
    /// Nobody can make it. Buying is the route.
    Buy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    pub reagent: String,
    pub short_by: u32,
}

/// One instruction: this character needs this, by this level, and here is who.
#[derive(Debug, Clone, PartialEq)]
pub struct Supply {
    /// Who needs it.
    pub character: String,
    pub key: String,
    pub character_level: Option<u16>,
    /// The milestone's short name, e.g. "A better wand".
    pub what: &'static str,
    pub why: &'static str,
    /// The level by which it should be in hand.
    pub needed_by_level: u16,
    /// True when the character is already past that level, so this is late
    /// rather than upcoming.
    pub overdue: bool,
    pub route: Route,
    pub source: &'static str,
    pub checked_for_forever: bool,
}

impl Supply {
    /// Whether anything has to happen before the item can exist.
    pub fn needs_preparation(&self) -> bool {
        matches!(
            self.route,
            Route::ShortOfMaterials { .. }
                | Route::ShortOfSkill { .. }
                | Route::NotLearned { .. }
                | Route::MaybeCapable { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProvisioningPlan {
    /// Most urgent first: overdue before upcoming, then by level.
    pub supplies: Vec<Supply>,
    pub caveats: Vec<String>,
}

/// What a character can do with one profession.
struct Crafter<'a> {
    name: &'a str,
    market: Option<String>,
    faction: Option<&'a str>,
    rank: Option<u16>,
    /// None when the trade window has never been opened for this profession.
    /// Not the same as knowing no recipes, and the two must not be merged:
    /// one means "go and learn it", the other means "nobody has looked".
    known_recipes: Option<Vec<String>>,
}

fn crafters_for<'a>(entries: &'a [RosterEntry<'a>], profession: &str) -> Vec<Crafter<'a>> {
    let mut found = Vec::new();
    for entry in entries {
        let record: &CharacterRecord = entry.record;

        let rank = record
            .professions
            .iter()
            .flatten()
            .find(|slot| {
                slot.name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(profession))
            })
            .and_then(|slot| slot.rank);

        let cache = record
            .recipes
            .iter()
            .flatten()
            .find(|(name, _)| name.eq_ignore_ascii_case(profession));

        // Has the profession at all? Either the client listed it, or a trade
        // window for it was scraped.
        if rank.is_none() && cache.is_none() {
            continue;
        }

        let known_recipes = cache.map(|(_, data)| {
            data.recipes
                .iter()
                .flatten()
                .filter_map(|recipe| recipe.name.clone())
                .collect()
        });

        found.push(Crafter {
            name: entry.name(),
            market: auctionator::realm_key(record.realm.as_deref(), record.faction.as_deref()),
            faction: record.faction.as_deref(),
            // The trade window's own rank is the fresher of the two when both
            // exist, because it was read from the window rather than a list.
            rank: cache.and_then(|(_, data)| data.rank).or(rank),
            known_recipes,
        });
    }
    found
}

/// Decide the single best route for one need.
///
/// Ordered by how little has to happen: made now beats made after shopping,
/// which beats made after levelling a profession, which beats bought. The
/// across-the-line answer sits just above "buy", because it is information
/// about the roster rather than an instruction.
fn route_for(
    want: &CraftRoute,
    needer: &str,
    needer_market: &Option<String>,
    crafters: &[Crafter<'_>],
    assessments: &[crafting::Craftable],
) -> Route {
    // What crafting already worked out for this recipe, in this market. It has
    // counted the roster's bags and banks; nothing here recounts them. Asked
    // for by name rather than taken from the craft plan, because that plan
    // drops anything with no profit in it — which is most of what somebody
    // needs for themselves.
    let ready = assessments
        .iter()
        .filter(|craftable| craftable.market == *needer_market);

    let mut short_of_materials: Option<Route> = None;

    for craftable in ready {
        if craftable.can_make_now > 0 {
            return if craftable.crafter == needer {
                Route::Themselves {
                    recipe: craftable.recipe.clone(),
                    can_make_now: craftable.can_make_now,
                }
            } else {
                Route::RosterCrafter {
                    crafter: craftable.crafter.clone(),
                    recipe: craftable.recipe.clone(),
                    can_make_now: craftable.can_make_now,
                }
            };
        }

        if short_of_materials.is_none() {
            let missing: Vec<Missing> = craftable
                .reagents
                .iter()
                .filter(|reagent| reagent.held < reagent.needed)
                .map(|reagent| Missing {
                    reagent: reagent
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("item {}", reagent.item_id)),
                    short_by: reagent.needed - reagent.held,
                })
                .collect();
            if !missing.is_empty() {
                short_of_materials = Some(Route::ShortOfMaterials {
                    crafter: craftable.crafter.clone(),
                    recipe: craftable.recipe.clone(),
                    missing,
                });
            }
        }
    }

    if let Some(route) = short_of_materials {
        return route;
    }

    // Nobody can make it today. The useful answer is now about who is closest,
    // and that is a question about skill rather than about materials.
    let mut best_skill_gap: Option<(u16, &Crafter<'_>)> = None;
    let mut not_learned: Option<&Crafter<'_>> = None;
    let mut maybe_capable: Option<&Crafter<'_>> = None;
    let mut across_the_line: Option<&Crafter<'_>> = None;

    for crafter in crafters {
        if crafter.market != *needer_market {
            // Right profession, wrong side of the world.
            if across_the_line.is_none() {
                across_the_line = Some(crafter);
            }
            continue;
        }

        let rank = crafter.rank.unwrap_or(0);
        if rank < want.needs_skill {
            let gap = want.needs_skill - rank;
            // `map_or`, not `is_none_or`: the latter is stable from 1.82 and this
            // crate's MSRV is 1.77.
            if best_skill_gap.map_or(true, |(best, _)| gap < best) {
                best_skill_gap = Some((gap, crafter));
            }
            continue;
        }

        // Skilled enough. Either the recipe is not learned, or the trade
        // window has never been opened and we genuinely do not know.
        match &crafter.known_recipes {
            Some(known)
                if not_learned.is_none()
                    && !known
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(want.recipe)) =>
            {
                not_learned = Some(crafter);
            }
            // Skilled enough, and nobody has looked at what they know.
            None if maybe_capable.is_none() => maybe_capable = Some(crafter),
            _ => {}
        }
    }

    if let Some(crafter) = not_learned {
        return Route::NotLearned {
            crafter: crafter.name.to_string(),
            profession: want.profession.to_string(),
            recipe: want.recipe.to_string(),
        };
    }

    if let Some((_, crafter)) = best_skill_gap {
        return Route::ShortOfSkill {
            crafter: crafter.name.to_string(),
            profession: want.profession.to_string(),
            recipe: want.recipe.to_string(),
            have: crafter.rank.unwrap_or(0),
            need: want.needs_skill,
        };
    }

    if let Some(crafter) = maybe_capable {
        return Route::MaybeCapable {
            crafter: crafter.name.to_string(),
            profession: want.profession.to_string(),
            rank: crafter.rank.unwrap_or(0),
        };
    }

    if let Some(crafter) = across_the_line {
        return Route::WrongFaction {
            crafter: crafter.name.to_string(),
            faction: crafter.faction.unwrap_or("the other faction").to_string(),
            recipe: want.recipe.to_string(),
        };
    }

    Route::Buy
}

/// Build the roster's provisioning plan.
///
/// Holdings are passed in rather than surveyed here, the same way
/// [`crate::crafting::plan`] takes them: this module joins, it does not
/// gather.
pub fn plan(
    entries: &[RosterEntry<'_>],
    holdings: &[Holding],
    prices: &PriceDb,
    lookahead: u16,
) -> ProvisioningPlan {
    let mut supplies = Vec::new();
    let mut caveats = Vec::new();
    let mut unlooked: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for entry in entries {
        let record = entry.record;
        let name = entry.name();
        // Every rung, not the narrowed one-per-ladder view: planning ahead
        // means a caster with no wand at 11 hears both "get one now" and "the
        // upgrade lands at 13", and the screen's single-rung view can only say
        // one of those.
        let statuses = gear::statuses(record);
        if statuses.is_empty() {
            continue;
        }

        let market = auctionator::realm_key(record.realm.as_deref(), record.faction.as_deref());
        let level = record.level;

        for status in &statuses {
            // Due now, or close enough that preparing is the point.
            let (needed_by, overdue) = match &status.standing {
                Standing::Due => (status.from_level, true),
                Standing::NotYet { at_level } => {
                    let within =
                        level.is_some_and(|have| at_level.saturating_sub(have) <= lookahead);
                    if !within {
                        continue;
                    }
                    (*at_level, false)
                }
                // Already carrying it, or the answer cannot be decided. Neither
                // is a job for somebody else.
                Standing::Met | Standing::Unknowable { .. } => continue,
            };

            let milestone = gear::MILESTONES
                .iter()
                .find(|candidate| candidate.what == status.what);
            let route = match milestone.and_then(|milestone| milestone.craft.as_ref()) {
                Some(want) => {
                    let crafters = crafters_for(entries, want.profession);
                    // A crafter nobody has opened the trade window for cannot
                    // be said to know or not know a recipe. Collect those so
                    // the plan can admit the blind spot instead of concluding
                    // past it.
                    for crafter in &crafters {
                        if crafter.known_recipes.is_none() && crafter.market == market {
                            unlooked
                                .entry(crafter.name.to_string())
                                .or_default()
                                .push(want.profession.to_string());
                        }
                    }
                    let assessments = crafting::for_recipe(entries, holdings, prices, want.recipe);
                    route_for(want, name, &market, &crafters, &assessments)
                }
                None => Route::Buy,
            };

            supplies.push(Supply {
                character: name.to_string(),
                key: entry.key.to_string(),
                character_level: level,
                what: status.what,
                why: status.why,
                needed_by_level: needed_by,
                overdue,
                route,
                source: status.source,
                checked_for_forever: status.checked_for_forever,
            });
        }
    }

    // Overdue first, then by how soon it is wanted.
    supplies.sort_by(|a, b| {
        b.overdue
            .cmp(&a.overdue)
            .then(a.needed_by_level.cmp(&b.needed_by_level))
            .then(a.character.cmp(&b.character))
    });

    for (crafter, professions) in unlooked {
        let mut list: Vec<String> = professions;
        list.sort();
        list.dedup();
        caveats.push(format!(
            "No trade window has been opened for {}'s {}, so which recipes they know is \
             unknown rather than empty. Open it in game once and this gets sharper.",
            crafter,
            list.join(" or ")
        ));
    }

    ProvisioningPlan { supplies, caveats }
}
