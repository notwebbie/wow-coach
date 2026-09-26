//! Who can make it, where the materials are, and whether it is worth doing.
//!
//! This is the question an alt army exists to answer and that no single
//! character can. The tailor knows the recipe, the cloth is on the bank alt,
//! and the only way to find that out in game is to log in on each character in
//! turn and remember what you saw. The roster already holds all of it.
//!
//! Three separate claims live here, and the module is careful never to let one
//! of them borrow confidence from another:
//!
//! - **Who can make it** — a fact, from the recipe cache.
//! - **Whether the materials exist on the roster** — a fact, from bags.
//! - **Whether it is worth making** — an estimate, from auction prices, with
//!   every weakness those prices have.
//!
//! A craft can be entirely makeable and worth nothing, or hugely profitable
//! and missing half its reagents. Those are different answers and they are
//! reported separately.
//!
//! # What a profit figure here is not
//!
//! It is not money. It is the difference between what the output last sold for
//! and what the reagents last sold for, before the auction house's cut, before
//! whether anyone is buying, and assuming the reagents would otherwise have
//! been sold rather than used. Materials already sitting in a bag have no cash
//! cost at all, which is why "what it would cost to buy the missing pieces" is
//! reported apart from "what the whole thing is nominally worth".

use std::collections::BTreeMap;

use crate::auctionator::{self, PriceDb, RealmPrices};
use crate::collector::Recipe;
use crate::economy::Holding;
use crate::roster::{Role, RosterEntry};

/// Where a reagent is, and whether there is enough of it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReagentStatus {
    pub item_id: u32,
    pub name: Option<String>,
    /// How many one craft needs.
    pub needed: u32,
    /// How many the roster holds, in this market.
    pub held: u32,
    /// Character name to count, so the player knows where to go.
    pub held_by: BTreeMap<String, u32>,
    /// Unit price, when the market has seen one.
    pub unit_price: Option<u64>,
    /// True when this slot accepts several interchangeable items and only the
    /// first was recorded — so a shortfall here may not be a real one.
    pub has_substitutes: bool,
}

impl ReagentStatus {
    pub fn short_by(&self) -> u32 {
        self.needed.saturating_sub(self.held)
    }

    pub fn is_satisfied(&self) -> bool {
        self.held >= self.needed
    }
}

/// One recipe, one crafter, one market.
#[derive(Debug, Clone, PartialEq)]
pub struct Craftable {
    pub recipe: String,
    pub profession: String,
    /// Who knows it.
    pub crafter: String,
    pub crafter_role: Role,
    /// Auctionator's realm key, because reagents and output price per faction.
    pub market: Option<String>,
    pub reagents: Vec<ReagentStatus>,
    /// The item produced, when the recipe makes one.
    pub output_item_id: Option<u32>,
    pub output_name: Option<String>,
    /// How many it makes, mid-range.
    pub output_count: Option<u32>,
    /// What the output last sold for, times the yield.
    pub output_value: Option<u64>,
    /// What the reagents last sold for, all of them, times what is needed.
    /// `None` when any reagent has no price — a partial total would read as a
    /// cheaper craft than it is.
    pub reagent_cost: Option<u64>,
    /// What it would cost to buy only the pieces the roster does not have.
    pub shortfall_cost: Option<u64>,
    /// How many complete crafts the roster's materials support.
    pub can_make_now: u32,
}

impl Craftable {
    /// Output value less reagent cost, when both are known.
    ///
    /// Negative is a real and common answer: on a healthy auction house most
    /// intermediate goods sell for less than their inputs, and a tool that
    /// hides that is worse than no tool.
    pub fn margin(&self) -> Option<i64> {
        match (self.output_value, self.reagent_cost) {
            (Some(value), Some(cost)) => Some(value as i64 - cost as i64),
            _ => None,
        }
    }

    /// Every reagent the roster is short of.
    pub fn missing(&self) -> Vec<&ReagentStatus> {
        self.reagents
            .iter()
            .filter(|reagent| !reagent.is_satisfied())
            .collect()
    }

    /// True when the whole recipe could be made right now from what is held.
    pub fn is_makeable(&self) -> bool {
        self.can_make_now > 0
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CraftPlan {
    /// Makeable right now from materials the roster holds, best margin first.
    pub ready: Vec<Craftable>,
    /// Known, priced and profitable, but short of something.
    pub worth_shopping_for: Vec<Craftable>,
    pub caveats: Vec<String>,
}

/// How many recipes to consider before giving up on ranking them all. A
/// maxed profession knows hundreds, and every one is a lookup against every
/// holding.
const MAX_RECIPES: usize = 5000;

/// Work out what the roster can make.
///
/// `holdings` comes from [`crate::economy::survey`], so materials are already
/// totalled per market and per item. Passing them in rather than recomputing
/// keeps one answer to "what does the roster hold" rather than two.
pub fn plan(entries: &[RosterEntry<'_>], holdings: &[Holding], prices: &PriceDb) -> CraftPlan {
    let mut plan = CraftPlan::default();

    // Holdings keyed by (market, item) so a reagent lookup cannot cross a
    // faction boundary. Two auction houses are two economies; cloth on an
    // Alliance alt cannot be posted to a Horde crafter.
    let mut held: BTreeMap<(Option<String>, u32), &Holding> = BTreeMap::new();
    for holding in holdings {
        held.insert((holding.market.clone(), holding.item_id), holding);
    }

    let mut considered = 0usize;
    let mut recipes_without_reagents = 0usize;

    for entry in entries {
        if !entry.role.in_economy() {
            continue;
        }
        let record = entry.record;
        let crafter = entry.name().to_string();
        let market = auctionator::realm_key(record.realm.as_deref(), record.faction.as_deref());
        let realm = market.as_ref().and_then(|key| prices.realms.get(key));

        for (profession, cache) in record.recipes.iter().flatten() {
            for recipe in cache.recipes.iter().flatten() {
                if considered >= MAX_RECIPES {
                    break;
                }
                considered += 1;
                let Some(name) = recipe.name.clone() else {
                    continue;
                };
                let Some(reagents) = recipe.reagents.as_ref().filter(|list| !list.is_empty())
                else {
                    // Known but with no reagents captured. Reporting it as
                    // makeable would be a guess dressed as a fact, and
                    // reporting it as short of everything would be worse.
                    recipes_without_reagents += 1;
                    continue;
                };

                let craftable = assess(
                    &name, profession, &crafter, entry.role, &market, realm, recipe, reagents,
                    &held,
                );
                if craftable.is_makeable() {
                    plan.ready.push(craftable);
                } else if craftable.margin().is_some_and(|margin| margin > 0) {
                    plan.worth_shopping_for.push(craftable);
                }
            }
        }
    }

    // Best margin first, then the ones that are simply makeable. A craft with
    // no price is still worth knowing you can make.
    plan.ready.sort_by(|a, b| {
        b.margin()
            .unwrap_or(i64::MIN)
            .cmp(&a.margin().unwrap_or(i64::MIN))
            .then_with(|| b.can_make_now.cmp(&a.can_make_now))
            .then_with(|| a.recipe.cmp(&b.recipe))
    });
    plan.worth_shopping_for.sort_by(|a, b| {
        b.margin()
            .unwrap_or(0)
            .cmp(&a.margin().unwrap_or(0))
            .then_with(|| a.missing().len().cmp(&b.missing().len()))
            .then_with(|| a.recipe.cmp(&b.recipe))
    });

    plan.caveats = caveats(&plan, recipes_without_reagents, considered);
    plan
}

#[allow(clippy::too_many_arguments)]
fn assess(
    name: &str,
    profession: &str,
    crafter: &str,
    role: Role,
    market: &Option<String>,
    realm: Option<&RealmPrices>,
    recipe: &Recipe,
    reagents: &[crate::collector::Reagent],
    held: &BTreeMap<(Option<String>, u32), &Holding>,
) -> Craftable {
    let mut statuses = Vec::new();
    // Starts unbounded and is narrowed by each reagent. A recipe whose
    // reagents are all satisfied many times over can still only be made as
    // often as its scarcest input allows.
    let mut can_make = u32::MAX;
    let mut cost: Option<u64> = Some(0);
    let mut shortfall: Option<u64> = Some(0);

    for reagent in reagents {
        let (Some(item_id), Some(needed)) = (reagent.item_id, reagent.count) else {
            // A reagent we cannot identify makes every count below a guess.
            cost = None;
            shortfall = None;
            can_make = 0;
            continue;
        };
        let needed = needed.max(1);
        let holding = held.get(&(market.clone(), item_id));
        let count = holding.map_or(0, |holding| holding.count);
        let unit_price = realm
            .and_then(|realm| realm.for_item(item_id))
            .and_then(|prices| prices.last_seen_min)
            .filter(|price| *price > 0);

        can_make = can_make.min(count / needed);

        match unit_price {
            Some(price) => {
                cost =
                    cost.map(|total| total.saturating_add(price.saturating_mul(u64::from(needed))));
                let short = needed.saturating_sub(count);
                shortfall = shortfall
                    .map(|total| total.saturating_add(price.saturating_mul(u64::from(short))));
            }
            None => {
                // One unpriced reagent makes the whole total a guess. Skipping
                // it would quietly report the craft as cheaper than it is.
                cost = None;
                shortfall = None;
            }
        }

        statuses.push(ReagentStatus {
            item_id,
            name: reagent
                .name
                .clone()
                .or_else(|| holding.and_then(|holding| holding.name.clone())),
            needed,
            held: count,
            held_by: holding
                .map(|holding| holding.held_by.clone())
                .unwrap_or_default(),
            unit_price,
            has_substitutes: reagent.choices.is_some_and(|choices| choices > 1),
        });
    }

    if statuses.is_empty() {
        can_make = 0;
    }

    let output_count = recipe.makes_typical();
    let output_holding = recipe
        .makes_item_id
        .and_then(|item_id| held.get(&(market.clone(), item_id)));
    let output_unit = recipe
        .makes_item_id
        .and_then(|item_id| realm.and_then(|realm| realm.for_item(item_id)))
        .and_then(|prices| prices.last_seen_min)
        .filter(|price| *price > 0);

    Craftable {
        recipe: name.to_string(),
        profession: profession.to_string(),
        crafter: crafter.to_string(),
        crafter_role: role,
        market: market.clone(),
        output_item_id: recipe.makes_item_id,
        output_name: output_holding.and_then(|holding| holding.name.clone()),
        output_count,
        output_value: output_unit
            .map(|price| price.saturating_mul(u64::from(output_count.unwrap_or(1)))),
        reagent_cost: cost,
        shortfall_cost: shortfall,
        can_make_now: if can_make == u32::MAX { 0 } else { can_make },
        reagents: statuses,
    }
}

fn caveats(plan: &CraftPlan, without_reagents: usize, considered: usize) -> Vec<String> {
    let mut caveats = Vec::new();

    if considered == 0 {
        caveats.push(
            "No recipes have been captured. Open each profession window once in game, then log out."
                .to_string(),
        );
        return caveats;
    }

    if without_reagents > 0 {
        caveats.push(format!(
            "{without_reagents} known recipe(s) have no reagents recorded, so nothing can be said about making them. Re-open those profession windows with a current collector installed."
        ));
    }

    let unpriced = plan
        .ready
        .iter()
        .filter(|craftable| craftable.reagent_cost.is_none())
        .count();
    if unpriced > 0 {
        caveats.push(format!(
            "{unpriced} craft(s) have a reagent the auction house has never shown, so they are listed as makeable but not valued. An unpriced reagent is not a free one."
        ));
    }

    if plan
        .ready
        .iter()
        .chain(&plan.worth_shopping_for)
        .any(|craftable| craftable.reagents.iter().any(|r| r.has_substitutes))
    {
        caveats.push(
            "Some recipes accept interchangeable reagents and only the first was recorded, so a shortfall on those may not be real."
                .to_string(),
        );
    }

    if plan
        .ready
        .iter()
        .any(|craftable| craftable.margin().is_some())
    {
        caveats.push(
            "Margins are last seen buyout for the output less last seen buyout for the reagents, before the auction house's cut. Materials already in your bags cost you nothing to use."
                .to_string(),
        );
    }

    caveats.push(
        "Only reagents in bags count. A bank the collector has not seen is invisible here."
            .to_string(),
    );

    caveats
}
