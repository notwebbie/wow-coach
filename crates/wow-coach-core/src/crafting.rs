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
use crate::economy::{HeldAmount, Holding};
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
    /// Character name to what it holds and where, so the player knows where to
    /// go and whether a bank trip is part of the job.
    pub held_by: BTreeMap<String, HeldAmount>,
    /// Unit price, when the market has seen one.
    pub unit_price: Option<u64>,
    /// The middle of this reagent's recent daily prices. One odd listing
    /// cannot move it, which is the whole point of carrying it alongside.
    pub typical_price: Option<u64>,
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

    /// How many of these are sitting in a bank rather than in bags.
    pub fn banked(&self) -> u32 {
        self.held_by.values().map(|held| held.bank).sum()
    }

    /// True when the craft only works after a trip to the bank: there is
    /// enough in total, but not enough to hand.
    ///
    /// Kept separate from a shortfall on purpose. "You cannot make this" and
    /// "you can make this once you fetch the linen" are different answers, and
    /// collapsing them into one loses the only part the player can act on.
    pub fn needs_a_bank_trip(&self) -> bool {
        let in_bags: u32 = self.held_by.values().map(|held| held.bags).sum();
        self.is_satisfied() && in_bags < self.needed
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
    /// The same at each reagent's typical price.
    ///
    /// A wrong reagent price does not merely misestimate a craft, it inverts
    /// the decision — and in the direction nobody checks, because a craft that
    /// looks unprofitable is simply never made. Live data: an Empty Vial
    /// listed at 24s against a usual 1s25c made every low-level potion look
    /// 24 silver worse than it was.
    pub reagent_cost_typical: Option<u64>,
    /// What it would cost to buy only the pieces the roster does not have.
    pub shortfall_cost: Option<u64>,
    /// How many complete crafts the roster's materials support.
    pub can_make_now: u32,
}

impl Craftable {
    /// Output value less reagent cost at each reagent's typical price.
    pub fn margin_typical(&self) -> Option<i64> {
        match (self.output_value, self.reagent_cost_typical) {
            (Some(value), Some(cost)) => Some(value as i64 - cost as i64),
            _ => None,
        }
    }

    /// True when costing at typical prices rather than last seen ones flips
    /// the answer from profit to loss or back.
    ///
    /// This is the test that matters, and it needs no threshold to argue
    /// about: a price is only misleading here if it changes what a reader
    /// would do. A craft that is unprofitable either way does not need a
    /// warning, however far the prices have moved.
    pub fn price_uncertainty_flips_the_answer(&self) -> bool {
        match (self.margin(), self.margin_typical()) {
            (Some(seen), Some(typical)) => (seen >= 0) != (typical >= 0),
            _ => false,
        }
    }

    /// The reagents whose last seen price is out of line with their own
    /// history, worst first — the ones carrying the disagreement.
    pub fn suspect_reagents(&self) -> Vec<&ReagentStatus> {
        let mut found: Vec<&ReagentStatus> = self
            .reagents
            .iter()
            .filter(
                |reagent| match (reagent.unit_price, reagent.typical_price) {
                    (Some(seen), Some(typical)) if typical > 0 => {
                        seen as f64 / typical as f64 >= 2.0
                    }
                    _ => false,
                },
            )
            .collect();
        found.sort_by(|a, b| {
            let ratio = |r: &ReagentStatus| {
                r.unit_price.unwrap_or(0) as f64 / r.typical_price.unwrap_or(1).max(1) as f64
            };
            ratio(b)
                .partial_cmp(&ratio(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        found
    }

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
    let mut cost_typical: Option<u64> = Some(0);
    let mut shortfall: Option<u64> = Some(0);

    for reagent in reagents {
        let (Some(item_id), Some(needed)) = (reagent.item_id, reagent.count) else {
            // A reagent we cannot identify makes every count below a guess.
            cost = None;
            cost_typical = None;
            shortfall = None;
            can_make = 0;
            continue;
        };
        let needed = needed.max(1);
        let holding = held.get(&(market.clone(), item_id));
        let count = holding.map_or(0, |holding| holding.count);
        let prices = realm.and_then(|realm| realm.for_item(item_id));
        let unit_price = prices
            .and_then(|prices| prices.last_seen_min)
            .filter(|price| *price > 0);
        let typical_price = prices
            .and_then(|prices| prices.typical())
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

        // The typical-price total falls back to the last seen price where
        // there is no history, so both totals cover the same reagents and can
        // honestly be compared.
        match typical_price.or(unit_price) {
            Some(price) => {
                cost_typical = cost_typical
                    .map(|total| total.saturating_add(price.saturating_mul(u64::from(needed))));
            }
            None => cost_typical = None,
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
            typical_price,
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
        reagent_cost_typical: cost_typical,
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

    let flipped: Vec<&Craftable> = plan
        .ready
        .iter()
        .chain(&plan.worth_shopping_for)
        .filter(|craftable| craftable.price_uncertainty_flips_the_answer())
        .collect();
    if !flipped.is_empty() {
        let mut blamed: Vec<&str> = flipped
            .iter()
            .flat_map(|craftable| craftable.suspect_reagents())
            .filter_map(|reagent| reagent.name.as_deref())
            .collect();
        blamed.sort_unstable();
        blamed.dedup();
        caveats.push(format!(
            "{} craft(s) turn from profit to loss, or back, depending on whether their reagents are costed at the last seen price or the usual one{}. An oddly priced reagent does not merely skew the figure — it reverses the decision, in the direction nobody checks, because a craft that looks unprofitable is simply never made.",
            flipped.len(),
            if blamed.is_empty() {
                String::new()
            } else {
                format!(" — {}", blamed.join(", "))
            }
        ));
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

    let bank_trips = plan
        .ready
        .iter()
        .filter(|craftable| {
            craftable
                .reagents
                .iter()
                .any(ReagentStatus::needs_a_bank_trip)
        })
        .count();
    if bank_trips > 0 {
        caveats.push(format!(
            "{bank_trips} craft(s) are only ready once you visit the bank — the reagents are owned but not to hand, and on another character that means the mailbox too."
        ));
    }

    caveats.push(
        "Bank contents are whatever was there the last time that character stood at a banker, so a stale bank reads as a holding you may have since spent."
            .to_string(),
    );

    caveats
}
