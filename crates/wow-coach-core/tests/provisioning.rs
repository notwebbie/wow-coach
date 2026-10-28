//! Tests for the roster provisioning plan.
//!
//! The scenario these are written around is the one that prompted the feature:
//! a mage who will want a better wand in a couple of levels, and an enchanter
//! elsewhere in the roster who may or may not be able to supply one. The
//! interesting cases are all the ways "may not" can be true, because each
//! sends the player somewhere different.

use std::collections::BTreeMap;

use wow_coach_core::auctionator::PriceDb;
use wow_coach_core::collector::{
    CharacterRecord, Equipment, EquippedItem, ProfessionRecipes, ProfessionSlot, Reagent, Recipe,
};
use wow_coach_core::economy;
use wow_coach_core::provisioning::{self, Route};
use wow_coach_core::roster::{self, Role, RosterConfig};

const SIMPLE_WOOD: u32 = 4470;
const LESSER_MAGIC_ESSENCE: u32 = 10938;

/// A character with equipment captured, so the gear layer will speak at all.
fn character(name: &str, class: &str, level: u16, faction: &str) -> CharacterRecord {
    CharacterRecord {
        name: Some(name.to_string()),
        class: Some(class.to_string()),
        level: Some(level),
        realm: Some("Spineshatter".to_string()),
        faction: Some(faction.to_string()),
        game_flavor: Some("forever".to_string()),
        equipment: Some(Equipment {
            // A staff and nothing else: no wand, which is the whole point.
            slots: Some(vec![EquippedItem {
                slot: Some(16),
                item_id: Some(5265),
                name: Some("Ashwood Staff".to_string()),
                item_type: Some("Weapon".to_string()),
                item_sub_type: Some("Staves".to_string()),
                equip_loc: Some("INVTYPE_2HWEAPON".to_string()),
                ..EquippedItem::default()
            }]),
            slots_read: Some(19),
        }),
        ..CharacterRecord::default()
    }
}

/// An enchanter at `rank`, knowing `recipes`, carrying `bag`.
fn enchanter(
    name: &str,
    faction: &str,
    rank: u16,
    recipes: &[&str],
    bag: &[(u32, u32)],
) -> CharacterRecord {
    let mut record = character(name, "WARLOCK", 20, faction);

    record.professions = Some(vec![ProfessionSlot {
        slot: Some(1),
        name: Some("Enchanting".to_string()),
        rank: Some(rank),
        max_rank: Some(300),
        ..ProfessionSlot::default()
    }]);
    record.professions_returned = Some(2);

    let mut cache = BTreeMap::new();
    cache.insert(
        "Enchanting".to_string(),
        ProfessionRecipes {
            rank: Some(rank),
            max_rank: Some(300),
            captured_at: Some(1_700_000_000),
            recipes: Some(
                recipes
                    .iter()
                    .map(|name| Recipe {
                        name: Some((*name).to_string()),
                        difficulty: Some("optimal".to_string()),
                        spell_id: Some(14807),
                        reagents: Some(vec![
                            Reagent {
                                item_id: Some(LESSER_MAGIC_ESSENCE),
                                name: Some("Lesser Magic Essence".to_string()),
                                count: Some(2),
                                choices: None,
                            },
                            Reagent {
                                item_id: Some(SIMPLE_WOOD),
                                name: Some("Simple Wood".to_string()),
                                count: Some(1),
                                choices: None,
                            },
                        ]),
                        ..Recipe::default()
                    })
                    .collect(),
            ),
        },
    );
    record.recipes = Some(cache);

    record.inventory = Some(wow_coach_core::collector::Inventory {
        bags: Some(vec![wow_coach_core::collector::Bag {
            bag_index: Some(0),
            slots: Some(16),
            free_slots: Some(10),
            ..Default::default()
        }]),
        total_slots: Some(16),
        free_slots: Some(10),
        contents: Some(
            bag.iter()
                .map(|(id, count)| wow_coach_core::collector::ItemStack {
                    item_id: Some(*id),
                    count: Some(*count),
                    name: None,
                })
                .collect(),
        ),
    });

    record
}

/// Build a roster, run the whole chain, and return the plan.
fn provision(members: Vec<(&str, CharacterRecord)>) -> provisioning::ProvisioningPlan {
    let mut characters = BTreeMap::new();
    let mut config = RosterConfig::default();
    for (key, record) in members {
        config.roles.insert(key.to_string(), Role::Active);
        characters.insert(key.to_string(), record);
    }

    let entries = roster::roster(&characters, &config);
    let prices = PriceDb::default();
    let survey = economy::survey(&entries, &prices);
    let holdings: Vec<_> = survey
        .holdings
        .iter()
        .chain(&survey.unpriced)
        .cloned()
        .collect();
    provisioning::plan(
        &entries,
        &holdings,
        &prices,
        provisioning::DEFAULT_LOOKAHEAD,
    )
}

/// The supply for one character's wand need at a given rung.
///
/// Named by rung rather than "the wand one", because a character short of a
/// wand entirely has two: get something now, and upgrade at 13. Collapsing
/// them in the helper hid a real behaviour the first time round.
fn wand_supply_at(
    plan: &provisioning::ProvisioningPlan,
    who: &str,
    what: &str,
) -> provisioning::Supply {
    plan.supplies
        .iter()
        .find(|supply| supply.character == who && supply.what == what)
        .unwrap_or_else(|| panic!("no '{what}' supply for {who}: {:#?}", plan.supplies))
        .clone()
}

fn wand_supply(plan: &provisioning::ProvisioningPlan, who: &str) -> provisioning::Supply {
    wand_supply_at(plan, who, "A better wand")
}

#[test]
fn an_enchanter_who_can_make_it_is_told_to_make_it() {
    // The headline case. One character, their own enchanting, materials in the
    // bag. The answer is not "go and buy a wand".
    let plan = provision(vec![(
        "7:forever|12:Spineshatter|6:Solitu",
        enchanter(
            "Solitu",
            "Horde",
            80,
            &["Greater Magic Wand"],
            &[(LESSER_MAGIC_ESSENCE, 4), (SIMPLE_WOOD, 2)],
        ),
    )]);

    let supply = wand_supply(&plan, "Solitu");
    match supply.route {
        Route::Themselves {
            ref recipe,
            can_make_now,
        } => {
            assert_eq!(recipe, "Greater Magic Wand");
            assert!(can_make_now >= 1, "the bag holds enough for one");
        }
        other => panic!("an enchanter with the mats should make it, not {other:?}"),
    }
}

#[test]
fn a_roster_enchanter_supplies_the_mage_on_the_same_side() {
    // The feature as asked for: the mage does not have Enchanting, the warlock
    // does, and they are both Horde.
    let plan = provision(vec![
        (
            "7:forever|12:Spineshatter|9:Gravehexx",
            character("Gravehexx", "MAGE", 11, "Horde"),
        ),
        (
            "7:forever|12:Spineshatter|7:Dardrin",
            enchanter(
                "Dardrin",
                "Horde",
                80,
                &["Greater Magic Wand"],
                &[(LESSER_MAGIC_ESSENCE, 4), (SIMPLE_WOOD, 2)],
            ),
        ),
    ]);

    let supply = wand_supply(&plan, "Gravehexx");
    match supply.route {
        Route::RosterCrafter { ref crafter, .. } => assert_eq!(crafter, "Dardrin"),
        other => panic!("the roster's own enchanter should supply it, not {other:?}"),
    }
}

#[test]
fn an_enchanter_on_the_other_faction_is_named_rather_than_ignored() {
    // Alliance enchanter, Horde mage. No mail, no auction house, no trade. The
    // wrong answer here is "buy one", because it hides that the player already
    // owns the capability and cannot reach it — which is a roster decision,
    // not a shopping one.
    let plan = provision(vec![
        (
            "7:forever|12:Spineshatter|9:Gravehexx",
            character("Gravehexx", "MAGE", 11, "Horde"),
        ),
        (
            "7:forever|12:Spineshatter|7:Alliane",
            enchanter(
                "Alliane",
                "Alliance",
                80,
                &["Greater Magic Wand"],
                &[(LESSER_MAGIC_ESSENCE, 4), (SIMPLE_WOOD, 2)],
            ),
        ),
    ]);

    let supply = wand_supply(&plan, "Gravehexx");
    match supply.route {
        Route::WrongFaction {
            ref crafter,
            ref faction,
            ..
        } => {
            assert_eq!(crafter, "Alliane");
            assert_eq!(faction, "Alliance");
        }
        other => panic!("the faction line should be named, not silently cost a route: {other:?}"),
    }
}

#[test]
fn a_skill_gap_is_reported_as_a_number_of_points() {
    // The second thing asked for: not "your enchanter cannot make this" but
    // "your enchanter is ten points short", which is a thing a player can act
    // on this evening.
    let plan = provision(vec![
        (
            "7:forever|12:Spineshatter|9:Gravehexx",
            character("Gravehexx", "MAGE", 11, "Horde"),
        ),
        (
            "7:forever|12:Spineshatter|7:Dardrin",
            enchanter("Dardrin", "Horde", 60, &[], &[]),
        ),
    ]);

    let supply = wand_supply(&plan, "Gravehexx");
    match supply.route {
        Route::ShortOfSkill {
            ref crafter,
            have,
            need,
            ref profession,
            ..
        } => {
            assert_eq!(crafter, "Dardrin");
            assert_eq!(profession, "Enchanting");
            assert_eq!(have, 60);
            assert_eq!(need, 70, "Greater Magic Wand is an Enchanting 70 craft");
        }
        other => panic!("ten points short is the actionable answer, not {other:?}"),
    }
    assert!(
        supply.needs_preparation(),
        "and it is flagged as something to do before the levelling session"
    );
}

#[test]
fn an_enchanter_who_has_the_skill_but_not_the_recipe_is_told_to_learn_it() {
    let plan = provision(vec![
        (
            "7:forever|12:Spineshatter|9:Gravehexx",
            character("Gravehexx", "MAGE", 11, "Horde"),
        ),
        (
            "7:forever|12:Spineshatter|7:Dardrin",
            enchanter("Dardrin", "Horde", 120, &["Runed Copper Rod"], &[]),
        ),
    ]);

    let supply = wand_supply(&plan, "Gravehexx");
    match supply.route {
        Route::NotLearned {
            ref crafter,
            ref recipe,
            ..
        } => {
            assert_eq!(crafter, "Dardrin");
            assert_eq!(recipe, "Greater Magic Wand");
        }
        other => panic!("skilled but untaught is its own answer: {other:?}"),
    }
}

#[test]
fn missing_reagents_are_named_with_the_shortfall() {
    let plan = provision(vec![(
        "7:forever|12:Spineshatter|6:Solitu",
        enchanter(
            "Solitu",
            "Horde",
            80,
            &["Greater Magic Wand"],
            &[(SIMPLE_WOOD, 2)],
        ),
    )]);

    let supply = wand_supply(&plan, "Solitu");
    match supply.route {
        Route::ShortOfMaterials { ref missing, .. } => {
            assert!(
                missing.iter().any(|m| m.reagent.contains("Essence")),
                "the missing reagent is named: {missing:?}"
            );
        }
        other => panic!("short of mats is its own answer: {other:?}"),
    }
}

#[test]
fn nobody_with_the_profession_at_all_means_buy_it() {
    let plan = provision(vec![(
        "7:forever|12:Spineshatter|9:Gravehexx",
        character("Gravehexx", "MAGE", 11, "Horde"),
    )]);

    let supply = wand_supply(&plan, "Gravehexx");
    assert_eq!(supply.route, Route::Buy);
}

#[test]
fn something_wanted_soon_is_raised_before_it_is_due() {
    // "Before the next levelling session" is the whole point: a wand wanted at
    // 13 is worth making at 11, not at 13.
    let plan = provision(vec![(
        "7:forever|12:Spineshatter|9:Gravehexx",
        character("Gravehexx", "MAGE", 11, "Horde"),
    )]);

    let upcoming = plan
        .supplies
        .iter()
        .find(|supply| supply.needed_by_level == 13)
        .expect("the level 13 wand is within the lookahead at level 11");
    assert!(
        !upcoming.overdue,
        "it is not late yet, and saying so would cry wolf"
    );
    assert_eq!(upcoming.character_level, Some(11));
}

#[test]
fn something_far_off_is_not_raised_yet() {
    let mut record = character("Newbie", "MAGE", 1, "Horde");
    record.level = Some(1);
    let plan = provision(vec![("7:forever|12:Spineshatter|6:Newbie", record)]);

    assert!(
        plan.supplies
            .iter()
            .all(|supply| supply.needed_by_level <= 6),
        "a level 1 character is not planning for level 13: {:#?}",
        plan.supplies
    );
}

#[test]
fn a_character_already_carrying_one_is_not_given_a_job() {
    let mut record = character("Gravehexx", "MAGE", 20, "Horde");
    record
        .equipment
        .as_mut()
        .unwrap()
        .slots
        .as_mut()
        .unwrap()
        .push(EquippedItem {
            slot: Some(18),
            item_id: Some(11288),
            name: Some("Greater Magic Wand".to_string()),
            item_level: Some(23),
            item_type: Some("Weapon".to_string()),
            item_sub_type: Some("Wands".to_string()),
            equip_loc: Some("INVTYPE_RANGEDRIGHT".to_string()),
            ..EquippedItem::default()
        });

    let plan = provision(vec![("7:forever|12:Spineshatter|9:Gravehexx", record)]);
    assert!(
        plan.supplies.is_empty(),
        "a wand is equipped; nothing to supply: {:#?}",
        plan.supplies
    );
}

#[test]
fn an_unopened_trade_window_is_admitted_rather_than_read_as_knowing_nothing() {
    // The enchanter's rank is known from the profession list, but no trade
    // window has ever been scraped, so which recipes they know is genuinely
    // unknown. Concluding "they have not learned it" would send the player to
    // a trainer to buy something they may already own.
    let mut record = enchanter("Dardrin", "Horde", 120, &[], &[]);
    record.recipes = None;

    let plan = provision(vec![
        (
            "7:forever|12:Spineshatter|9:Gravehexx",
            character("Gravehexx", "MAGE", 11, "Horde"),
        ),
        ("7:forever|12:Spineshatter|7:Dardrin", record),
    ]);

    assert!(
        plan.caveats
            .iter()
            .any(|caveat| caveat.contains("Dardrin") && caveat.contains("trade window")),
        "the blind spot is named: {:?}",
        plan.caveats
    );

    let supply = wand_supply(&plan, "Gravehexx");
    assert!(
        !matches!(supply.route, Route::NotLearned { .. }),
        "and no conclusion is drawn past it: {:?}",
        supply.route
    );
}

#[test]
fn the_unverified_flag_survives_the_join() {
    // Provenance has to cross two modules to reach a reader here. Each hop is
    // a place it can be dropped while both halves still look correct.
    let plan = provision(vec![(
        "7:forever|12:Spineshatter|9:Hunterman",
        character("Hunterman", "HUNTER", 11, "Horde"),
    )]);

    let supply = plan
        .supplies
        .iter()
        .find(|supply| supply.what.contains("ranged"))
        .expect("the hunter milestone applies");
    assert!(
        !supply.checked_for_forever,
        "the Classic-carryover flag reaches the far side of the join"
    );
    assert!(!supply.source.is_empty());
}

#[test]
fn overdue_work_sorts_above_upcoming_work() {
    let plan = provision(vec![
        (
            "7:forever|12:Spineshatter|9:Gravehexx",
            character("Gravehexx", "MAGE", 11, "Horde"),
        ),
        (
            "7:forever|12:Spineshatter|6:Latent",
            character("Latent", "MAGE", 30, "Horde"),
        ),
    ]);

    let first = plan.supplies.first().expect("something to do");
    assert!(
        first.overdue,
        "the level 30 mage with no wand at all comes before the level 11 one's \
         upcoming upgrade: {:#?}",
        plan.supplies
    );
}

#[test]
fn a_same_faction_crafter_outranks_one_across_the_line_even_when_less_is_known() {
    // Found by running the thing: a Horde mage was told an Alliance enchanter
    // could make a wand, while a Horde enchanter with ample skill stood right
    // there. He was skipped only because nobody had opened his trade window,
    // and the logic fell past him to an answer that is useless by definition.
    // Not knowing whether the reachable crafter can help beats knowing the
    // unreachable one can.
    let mut reachable = enchanter("Dardrin", "Horde", 120, &[], &[]);
    reachable.recipes = None;

    let plan = provision(vec![
        (
            "7:forever|12:Spineshatter|9:Gravehexx",
            character("Gravehexx", "MAGE", 11, "Horde"),
        ),
        ("7:forever|12:Spineshatter|7:Dardrin", reachable),
        (
            "7:forever|12:Spineshatter|9:Dawnholow",
            enchanter(
                "Dawnholow",
                "Alliance",
                150,
                &["Lesser Magic Wand", "Greater Magic Wand"],
                &[(LESSER_MAGIC_ESSENCE, 9), (SIMPLE_WOOD, 9)],
            ),
        ),
    ]);

    let supply = wand_supply_at(&plan, "Gravehexx", "A wand");
    match supply.route {
        Route::MaybeCapable {
            ref crafter, rank, ..
        } => {
            assert_eq!(crafter, "Dardrin");
            assert_eq!(rank, 120);
        }
        other => panic!("the reachable crafter is the lead, not {other:?}"),
    }
}
