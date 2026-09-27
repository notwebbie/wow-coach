//! Tests for the cross-character crafting view.
//!
//! The claim under test throughout: three separate questions — who can make
//! it, whether the materials exist, whether it is worth doing — and none of
//! them may borrow confidence from another.

use std::collections::BTreeMap;

use wow_coach_core::auctionator::{ItemPrices, PriceDb, RealmPrices};
use wow_coach_core::collector::{
    Bank, CharacterRecord, Inventory, ItemStack, ProfessionRecipes, Reagent, Recipe,
};
use wow_coach_core::crafting;
use wow_coach_core::economy;
use wow_coach_core::economy::HeldAmount;
use wow_coach_core::roster::{self, Role, RosterConfig};

const HORDE: &str = "Dreamscythe Horde";

fn character(name: &str, faction: &str, stacks: &[(u32, u32)]) -> CharacterRecord {
    CharacterRecord {
        name: Some(name.to_string()),
        level: Some(60),
        realm: Some("Dreamscythe".to_string()),
        faction: Some(faction.to_string()),
        money_copper: Some(0),
        inventory: Some(Inventory {
            bags: Some(Vec::new()),
            total_slots: Some(80),
            free_slots: Some(40),
            contents: Some(
                stacks
                    .iter()
                    .map(|(item_id, count)| ItemStack {
                        item_id: Some(*item_id),
                        count: Some(*count),
                        name: None,
                    })
                    .collect(),
            ),
        }),
        ..Default::default()
    }
}

/// The same character, with `banked` sitting in the bank as of `captured_at`.
fn with_bank(
    mut record: CharacterRecord,
    banked: &[(u32, u32)],
    captured_at: i64,
) -> CharacterRecord {
    record.bank = Some(Bank {
        bags: Some(Vec::new()),
        total_slots: Some(28),
        free_slots: Some(20),
        contents: Some(
            banked
                .iter()
                .map(|(item_id, count)| ItemStack {
                    item_id: Some(*item_id),
                    count: Some(*count),
                    name: None,
                })
                .collect(),
        ),
        captured_at: Some(captured_at),
    });
    record
}

/// `reagents` is (item id, count required).
fn recipe(name: &str, reagents: &[(u32, u32)], makes: Option<u32>, yield_each: u32) -> Recipe {
    Recipe {
        name: Some(name.to_string()),
        difficulty: Some("optimal".to_string()),
        spell_id: Some(11450),
        reagents: Some(
            reagents
                .iter()
                .map(|(item_id, count)| Reagent {
                    item_id: Some(*item_id),
                    name: None,
                    count: Some(*count),
                    choices: None,
                })
                .collect(),
        ),
        makes_item_id: makes,
        makes_min: Some(yield_each),
        makes_max: Some(yield_each),
    }
}

fn with_recipes(
    mut record: CharacterRecord,
    profession: &str,
    recipes: Vec<Recipe>,
) -> CharacterRecord {
    record.recipes.get_or_insert_with(BTreeMap::new).insert(
        profession.to_string(),
        ProfessionRecipes {
            captured_at: Some(1_800_000_000),
            rank: Some(300),
            max_rank: Some(300),
            recipes: Some(recipes),
        },
    );
    record
}

fn characters(records: Vec<CharacterRecord>) -> BTreeMap<String, CharacterRecord> {
    records
        .into_iter()
        .map(|record| (record.name.clone().unwrap(), record))
        .collect()
}

fn prices_for(realm: &str, entries: &[(u32, u64)]) -> PriceDb {
    let items = entries
        .iter()
        .map(|(item_id, price)| {
            (
                item_id.to_string(),
                ItemPrices {
                    last_seen_min: Some(*price),
                    high: [(2460, *price)].into_iter().collect(),
                    low: BTreeMap::new(),
                    available: BTreeMap::new(),
                },
            )
        })
        .collect();
    PriceDb {
        realms: [(realm.to_string(), RealmPrices { items })]
            .into_iter()
            .collect(),
        vendor: BTreeMap::new(),
        notes: Vec::new(),
    }
}

/// Run the economy survey then the craft plan, the way the CLI does.
fn plan_for(
    records: &BTreeMap<String, CharacterRecord>,
    config: &RosterConfig,
    prices: &PriceDb,
) -> crafting::CraftPlan {
    let entries = roster::roster(records, config);
    let survey = economy::survey(&entries, prices);
    let holdings: Vec<_> = survey
        .holdings
        .iter()
        .chain(&survey.unpriced)
        .cloned()
        .collect();
    crafting::plan(&entries, &holdings, prices)
}

#[test]
fn the_recipe_and_the_materials_can_live_on_different_characters() {
    // The headline claim. The tailor knows the recipe, the cloth is on the
    // bank alt, and no single character can see both.
    let records = characters(vec![
        with_recipes(
            character("Tailor", "Horde", &[]),
            "Tailoring",
            vec![recipe("Linen Bag", &[(2589, 4)], Some(4238), 1)],
        ),
        character("Vault", "Horde", &[(2589, 40)]),
    ]);
    let mut config = RosterConfig::default();
    config.set_role("Vault", Role::Bank);

    let plan = plan_for(
        &records,
        &config,
        &prices_for(HORDE, &[(2589, 100), (4238, 900)]),
    );

    assert_eq!(
        plan.ready.len(),
        1,
        "the craft is ready even though the crafter holds nothing"
    );
    let bag = &plan.ready[0];
    assert_eq!(bag.crafter, "Tailor");
    assert_eq!(bag.can_make_now, 10, "40 cloth at 4 each");
    assert_eq!(
        bag.reagents[0].held_by.get("Vault").map(HeldAmount::total),
        Some(40),
        "and it says the cloth is on the bank alt"
    );
    assert_eq!(bag.reagent_cost, Some(400));
    assert_eq!(bag.output_value, Some(900));
    assert_eq!(bag.margin(), Some(500));
}

#[test]
fn reagents_in_the_bank_count_toward_a_craft() {
    // The case the whole feature exists for. A bank alt's mats are in its
    // bank, not its bags — which is what a bank alt is — so a plan that only
    // reads bags reports "you cannot make this" about a craft the player has
    // every material for.
    let records = characters(vec![
        with_recipes(
            character("Tailor", "Horde", &[]),
            "Tailoring",
            vec![recipe("Linen Bag", &[(2589, 4)], Some(4238), 1)],
        ),
        with_bank(character("Vault", "Horde", &[]), &[(2589, 40)], 1_000),
    ]);
    let mut config = RosterConfig::default();
    config.set_role("Vault", Role::Bank);

    let plan = plan_for(
        &records,
        &config,
        &prices_for(HORDE, &[(2589, 100), (4238, 900)]),
    );

    assert_eq!(plan.ready.len(), 1, "the banked cloth is still cloth");
    let bag = &plan.ready[0];
    assert_eq!(bag.can_make_now, 10);

    let held = bag.reagents[0].held_by.get("Vault").expect("located");
    assert_eq!(held.bags, 0);
    assert_eq!(
        held.bank, 40,
        "and it is recorded as banked, not as to hand"
    );
    assert!(held.needs_a_trip());
    assert!(
        bag.reagents[0].needs_a_bank_trip(),
        "owning it and having it to hand are different answers"
    );
}

#[test]
fn a_reagent_split_across_bags_and_bank_needs_no_trip_when_the_bags_suffice() {
    // The distinction that stops the bank warning becoming noise. Enough in
    // the bags is enough, whatever else is sitting in the vault.
    let records = characters(vec![with_bank(
        with_recipes(
            character("Tailor", "Horde", &[(2589, 4)]),
            "Tailoring",
            vec![recipe("Linen Bag", &[(2589, 4)], Some(4238), 1)],
        ),
        &[(2589, 100)],
        1_000,
    )]);

    let plan = plan_for(
        &records,
        &RosterConfig::default(),
        &prices_for(HORDE, &[(2589, 100), (4238, 900)]),
    );

    let bag = &plan.ready[0];
    let held = bag.reagents[0].held_by.get("Tailor").expect("located");
    assert_eq!((held.bags, held.bank), (4, 100));
    assert_eq!(bag.can_make_now, 26, "104 cloth at 4 each");
    assert!(
        !bag.reagents[0].needs_a_bank_trip(),
        "the four in the bags are enough to start right now"
    );
    assert_eq!(bag.reagents[0].banked(), 100);
}

#[test]
fn a_bank_that_has_never_been_seen_is_reported_rather_than_assumed_empty() {
    // An unseen bank and an empty one produce the same number and mean
    // opposite things. Only one of them justifies telling the player to go
    // buy reagents.
    let records = characters(vec![character("Tailor", "Horde", &[(2589, 4)])]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(&entries, &prices_for(HORDE, &[(2589, 100)]));

    assert!(
        survey
            .caveats
            .iter()
            .any(|caveat| caveat.contains("bank has never been seen") && caveat.contains("Tailor")),
        "got {:?}",
        survey.caveats
    );
}

#[test]
fn a_bank_seen_and_empty_says_nothing() {
    // The other half of the same claim: once the player has stood at a banker,
    // silence is the correct output. A warning that never goes away is one
    // nobody reads.
    let records = characters(vec![with_bank(
        character("Tailor", "Horde", &[(2589, 4)]),
        &[],
        1_000,
    )]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(&entries, &prices_for(HORDE, &[(2589, 100)]));

    assert!(
        !survey
            .caveats
            .iter()
            .any(|caveat| caveat.contains("bank has never been seen")),
        "got {:?}",
        survey.caveats
    );
}

#[test]
fn the_scarcest_reagent_decides_how_many_can_be_made() {
    // Plenty of one thing and barely any of another is not "plenty".
    let records = characters(vec![with_recipes(
        character("Alch", "Horde", &[(2447, 100), (3356, 3)]),
        "Alchemy",
        vec![recipe("Elixir", &[(2447, 2), (3356, 1)], Some(3825), 1)],
    )]);
    let plan = plan_for(
        &records,
        &RosterConfig::default(),
        &prices_for(HORDE, &[(2447, 10), (3356, 500), (3825, 2000)]),
    );

    assert_eq!(
        plan.ready[0].can_make_now, 3,
        "three Goldthorn, three elixirs"
    );
}

#[test]
fn a_negative_margin_is_reported_rather_than_hidden() {
    // On a healthy auction house most intermediate goods sell for less than
    // their inputs. A tool that hides that is worse than no tool.
    let records = characters(vec![with_recipes(
        character("Smith", "Horde", &[(2840, 10)]),
        "Blacksmithing",
        vec![recipe("Copper Bar Thing", &[(2840, 5)], Some(2853), 1)],
    )]);
    let plan = plan_for(
        &records,
        &RosterConfig::default(),
        &prices_for(HORDE, &[(2840, 500), (2853, 900)]),
    );

    let item = &plan.ready[0];
    assert_eq!(item.reagent_cost, Some(2500));
    assert_eq!(item.output_value, Some(900));
    assert_eq!(item.margin(), Some(-1600), "losing money is a real answer");
}

#[test]
fn one_unpriced_reagent_makes_the_whole_cost_unknown() {
    // Skipping the unpriced one would report the craft as cheaper than it is,
    // which is the direction that loses money.
    let records = characters(vec![with_recipes(
        character("Alch", "Horde", &[(2447, 50), (99999, 50)]),
        "Alchemy",
        vec![recipe(
            "Mystery Brew",
            &[(2447, 1), (99999, 1)],
            Some(3825),
            1,
        )],
    )]);
    let plan = plan_for(
        &records,
        &RosterConfig::default(),
        &prices_for(HORDE, &[(2447, 10), (3825, 5000)]),
    );

    let brew = &plan.ready[0];
    assert!(brew.is_makeable(), "it can still be made");
    assert_eq!(brew.reagent_cost, None, "but not costed");
    assert_eq!(brew.margin(), None, "and so not valued");
    // The typical-price total has to fail the same way. A partial total there
    // would read as a cheaper craft just as surely, and it is the figure the
    // flip test compares against — so a hole in it would quietly reverse the
    // very decision that comparison exists to protect.
    assert_eq!(
        brew.reagent_cost_typical, None,
        "an unpriced reagent leaves no honest typical total either"
    );
    assert_eq!(brew.margin_typical(), None);
    assert!(
        !brew.price_uncertainty_flips_the_answer(),
        "and nothing can be said about a flip with no totals to compare"
    );
    assert!(
        plan.caveats
            .iter()
            .any(|caveat| caveat.contains("not a free one")),
        "{:?}",
        plan.caveats
    );
}

#[test]
fn materials_never_cross_a_faction_boundary() {
    // Two auction houses are two economies. Cloth on an Alliance alt cannot be
    // posted to a Horde crafter, so it must not satisfy a Horde recipe.
    let records = characters(vec![
        with_recipes(
            character("HordeTailor", "Horde", &[]),
            "Tailoring",
            vec![recipe("Linen Bag", &[(2589, 4)], Some(4238), 1)],
        ),
        character("AllyHoarder", "Alliance", &[(2589, 400)]),
    ]);
    let plan = plan_for(
        &records,
        &RosterConfig::default(),
        &prices_for(HORDE, &[(2589, 100), (4238, 900)]),
    );

    assert!(
        plan.ready.is_empty(),
        "400 cloth on the wrong faction is no cloth at all"
    );
}

#[test]
fn a_recipe_with_no_reagents_captured_is_not_claimed_either_way() {
    // "Known but unmeasured" is not "makeable" and not "short of everything".
    let mut bare = recipe("Unknown Reagents", &[], Some(3825), 1);
    bare.reagents = None;
    let records = characters(vec![with_recipes(
        character("Alch", "Horde", &[(2447, 100)]),
        "Alchemy",
        vec![bare],
    )]);
    let plan = plan_for(
        &records,
        &RosterConfig::default(),
        &prices_for(HORDE, &[(2447, 10), (3825, 5000)]),
    );

    assert!(plan.ready.is_empty());
    assert!(plan.worth_shopping_for.is_empty());
    assert!(
        plan.caveats
            .iter()
            .any(|caveat| caveat.contains("no reagents recorded")),
        "{:?}",
        plan.caveats
    );
}

#[test]
fn a_profitable_craft_you_are_short_of_is_a_shopping_list() {
    let records = characters(vec![with_recipes(
        character("Tailor", "Horde", &[(2589, 1)]),
        "Tailoring",
        vec![recipe("Linen Bag", &[(2589, 4)], Some(4238), 1)],
    )]);
    let plan = plan_for(
        &records,
        &RosterConfig::default(),
        &prices_for(HORDE, &[(2589, 100), (4238, 2000)]),
    );

    assert!(plan.ready.is_empty(), "one cloth of four is not makeable");
    assert_eq!(plan.worth_shopping_for.len(), 1);
    let bag = &plan.worth_shopping_for[0];
    assert_eq!(bag.missing().len(), 1);
    assert_eq!(bag.missing()[0].short_by(), 3);
    assert_eq!(
        bag.shortfall_cost,
        Some(300),
        "only the three you do not have, not all four"
    );
}

#[test]
fn a_bank_alt_can_be_the_crafter_as_well_as_the_warehouse() {
    // Bank alts are where professions get parked. Excluding them from crafting
    // would defeat the point of the role.
    let records = characters(vec![with_recipes(
        character("Vault", "Horde", &[(2589, 40)]),
        "Tailoring",
        vec![recipe("Linen Bag", &[(2589, 4)], Some(4238), 1)],
    )]);
    let mut config = RosterConfig::default();
    config.set_role("Vault", Role::Bank);

    let plan = plan_for(
        &records,
        &config,
        &prices_for(HORDE, &[(2589, 100), (4238, 900)]),
    );

    assert_eq!(plan.ready.len(), 1);
    assert_eq!(plan.ready[0].crafter_role, Role::Bank);
}

#[test]
fn a_recipe_that_makes_several_is_valued_for_all_of_them() {
    let records = characters(vec![with_recipes(
        character("Alch", "Horde", &[(2447, 10)]),
        "Alchemy",
        vec![recipe("Batch Brew", &[(2447, 1)], Some(3825), 5)],
    )]);
    let plan = plan_for(
        &records,
        &RosterConfig::default(),
        &prices_for(HORDE, &[(2447, 10), (3825, 100)]),
    );

    let brew = &plan.ready[0];
    assert_eq!(brew.output_count, Some(5));
    assert_eq!(brew.output_value, Some(500), "five at a hundred each");
    assert_eq!(brew.margin(), Some(490));
}

#[test]
fn a_recipe_with_no_output_item_is_still_reported_as_makeable() {
    // An enchant is applied to gear and produces nothing to sell. That makes
    // it unvaluable, not unmakeable.
    let records = characters(vec![with_recipes(
        character("Ench", "Horde", &[(10940, 30)]),
        "Enchanting",
        vec![recipe("Enchant Bracer", &[(10940, 3)], None, 1)],
    )]);
    let plan = plan_for(
        &records,
        &RosterConfig::default(),
        &prices_for(HORDE, &[(10940, 200)]),
    );

    let enchant = &plan.ready[0];
    assert_eq!(enchant.can_make_now, 10);
    assert_eq!(enchant.output_value, None);
    assert_eq!(
        enchant.margin(),
        None,
        "nothing to sell is not a margin of zero"
    );
    assert_eq!(enchant.reagent_cost, Some(600));
}

/// The holdings the economy survey would produce for these characters.
fn holdings_for(
    records: &BTreeMap<String, CharacterRecord>,
    prices: &PriceDb,
) -> Vec<economy::Holding> {
    let entries = roster::roster(records, &RosterConfig::default());
    let survey = economy::survey(&entries, prices);
    survey
        .holdings
        .iter()
        .chain(&survey.unpriced)
        .cloned()
        .collect()
}

/// One item's price: its id, what was last seen, and its daily history.
type PricedItem<'a> = (u32, u64, &'a [(i64, u64)]);

/// Prices where the last seen figure disagrees with the item's own history.
fn prices_with_history(realm: &str, entries: &[PricedItem<'_>]) -> PriceDb {
    let items = entries
        .iter()
        .map(|(item_id, last, history)| {
            (
                item_id.to_string(),
                ItemPrices {
                    last_seen_min: Some(*last),
                    high: history.iter().copied().collect(),
                    low: BTreeMap::new(),
                    available: BTreeMap::new(),
                },
            )
        })
        .collect();
    PriceDb {
        realms: [(realm.to_string(), RealmPrices { items })]
            .into_iter()
            .collect(),
        vendor: BTreeMap::new(),
        notes: Vec::new(),
    }
}

#[test]
fn one_odd_reagent_price_must_not_silently_reverse_the_answer() {
    // Live data, 28 September: an Empty Vial listed at 24s87c against a usual
    // 1s25c made every low-level potion look about 24 silver worse than it
    // was. A wrong reagent price does not merely skew a figure — it inverts
    // the decision, and in the direction nobody checks, because a craft that
    // looks unprofitable is simply never made.
    let records = characters(vec![with_recipes(
        character("Alch", "Horde", &[(2447, 10), (3371, 10)]),
        "Alchemy",
        vec![recipe("Potion", &[(2447, 1), (3371, 1)], Some(118), 1)],
    )]);
    let entries = roster::roster(&records, &RosterConfig::default());

    let prices = prices_with_history(
        HORDE,
        &[
            // Herb: steady.
            (2447, 8, &[(2458, 9), (2459, 9), (2460, 8)]),
            // Vial: one listing twenty times its usual price.
            (3371, 2487, &[(2458, 120), (2459, 131), (2460, 2487)]),
            // The potion sells for well over its usual reagent cost.
            (118, 400, &[(2458, 380), (2459, 400), (2460, 400)]),
        ],
    );
    let plan = crafting::plan(&entries, &holdings_for(&records, &prices), &prices);

    let potion = &plan.ready[0];
    assert_eq!(potion.reagent_cost, Some(2495), "what was last seen");
    assert_eq!(
        potion.reagent_cost_typical,
        Some(140),
        "what it usually costs"
    );
    assert!(
        potion.margin().unwrap() < 0,
        "looks like a loss at last seen prices"
    );
    assert!(
        potion.margin_typical().unwrap() > 0,
        "and a profit at usual ones"
    );
    assert!(
        potion.price_uncertainty_flips_the_answer(),
        "which is exactly the case worth warning about"
    );

    let suspect = potion.suspect_reagents();
    assert_eq!(suspect.len(), 1, "only the vial is out of line");
    assert_eq!(suspect[0].item_id, 3371);

    assert!(
        plan.caveats
            .iter()
            .any(|caveat| caveat.contains("reverses the decision")),
        "{:?}",
        plan.caveats
    );
}

#[test]
fn a_craft_that_loses_money_either_way_needs_no_warning() {
    // The test is whether the answer changes, not whether the prices moved.
    // Warning about a craft that is unprofitable on any costing would be the
    // crying-wolf failure this rule exists to avoid.
    let records = characters(vec![with_recipes(
        character("Alch", "Horde", &[(2447, 10), (3371, 10)]),
        "Alchemy",
        vec![recipe("Bad Potion", &[(2447, 1), (3371, 1)], Some(118), 1)],
    )]);
    let entries = roster::roster(&records, &RosterConfig::default());

    let prices = prices_with_history(
        HORDE,
        &[
            (2447, 8, &[(2459, 9), (2460, 8)]),
            (3371, 2487, &[(2459, 131), (2460, 2487)]),
            // Sells for 6 copper. Nothing makes this worth doing.
            (118, 6, &[(2459, 11), (2460, 6)]),
        ],
    );
    let plan = crafting::plan(&entries, &holdings_for(&records, &prices), &prices);

    let potion = &plan.ready[0];
    assert!(potion.margin().unwrap() < 0);
    assert!(potion.margin_typical().unwrap() < 0);
    assert!(
        !potion.price_uncertainty_flips_the_answer(),
        "a loss either way is not a warning"
    );
    assert!(!plan
        .caveats
        .iter()
        .any(|caveat| caveat.contains("reverses the decision")));
}
