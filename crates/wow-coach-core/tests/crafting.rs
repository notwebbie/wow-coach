//! Tests for the cross-character crafting view.
//!
//! The claim under test throughout: three separate questions — who can make
//! it, whether the materials exist, whether it is worth doing — and none of
//! them may borrow confidence from another.

use std::collections::BTreeMap;

use wow_coach_core::auctionator::{ItemPrices, PriceDb, RealmPrices};
use wow_coach_core::collector::{
    CharacterRecord, Inventory, ItemStack, ProfessionRecipes, Reagent, Recipe,
};
use wow_coach_core::crafting;
use wow_coach_core::economy;
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
        bag.reagents[0].held_by.get("Vault"),
        Some(&40),
        "and it says the cloth is on the bank alt"
    );
    assert_eq!(bag.reagent_cost, Some(400));
    assert_eq!(bag.output_value, Some(900));
    assert_eq!(bag.margin(), Some(500));
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
