//! Tests for profession levelling advice.
//!
//! The claim under test: this answers "what raises the skill", which is a
//! different question from "what is worth making", and the two must not be
//! allowed to borrow each other's answers. Plus the gates — a trainer that
//! cannot help yet is worse than no advice, because the player makes the trip.

use std::collections::BTreeMap;

use wow_coach_core::collector::{
    CharacterRecord, Inventory, ItemStack, ProfessionRecipes, Reagent, Recipe,
};
use wow_coach_core::economy;
use wow_coach_core::professions::{self, Difficulty, Training};
use wow_coach_core::roster::{self, RosterConfig};

/// The client the fixtures below are on. Spelled out because the tier table
/// is per-client: Master exists here and does not exist on Forever.
const TBC: Option<&str> = Some("tbc_classic");

fn recipe(name: &str, difficulty: &str, reagents: &[(u32, u32)]) -> Recipe {
    Recipe {
        name: Some(name.to_string()),
        difficulty: Some(difficulty.to_string()),
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
        ..Default::default()
    }
}

fn character(
    name: &str,
    level: u16,
    bags: &[(u32, u32)],
    profession: &str,
    rank: u16,
    max_rank: u16,
    recipes: Vec<Recipe>,
) -> CharacterRecord {
    let mut cache = BTreeMap::new();
    cache.insert(
        profession.to_string(),
        ProfessionRecipes {
            rank: Some(rank),
            max_rank: Some(max_rank),
            recipes: Some(recipes),
            ..Default::default()
        },
    );
    CharacterRecord {
        name: Some(name.to_string()),
        level: Some(level),
        realm: Some("Dreamscythe".to_string()),
        faction: Some("Horde".to_string()),
        money_copper: Some(0),
        inventory: Some(Inventory {
            bags: Some(Vec::new()),
            total_slots: Some(80),
            free_slots: Some(40),
            contents: Some(
                bags.iter()
                    .map(|(item_id, count)| ItemStack {
                        item_id: Some(*item_id),
                        count: Some(*count),
                        name: None,
                    })
                    .collect(),
            ),
        }),
        recipes: Some(cache),
        ..Default::default()
    }
}

fn plan_for(records: Vec<CharacterRecord>) -> Vec<professions::ProfessionPlan> {
    let characters: BTreeMap<String, CharacterRecord> = records
        .into_iter()
        .map(|record| (record.name.clone().unwrap_or_default(), record))
        .collect();
    let config = RosterConfig::default();
    let entries = roster::roster(&characters, &config);
    let survey = economy::survey(&entries, &Default::default());
    let holdings: Vec<_> = survey
        .holdings
        .iter()
        .chain(&survey.unpriced)
        .cloned()
        .collect();
    professions::plan(&entries, &holdings)
}

#[test]
fn a_grey_recipe_is_not_a_way_to_raise_the_skill() {
    // The headline. A trivial recipe can still be worth making for money, and
    // `craft` will happily rank it. It will never move the number, so it has
    // no business in a list of what to make next for skill.
    let plans = plan_for(vec![character(
        "Tailor",
        20,
        &[(2589, 100)],
        "Tailoring",
        60,
        75,
        vec![
            recipe("Linen Cloak", "trivial", &[(2589, 2)]),
            recipe("Linen Belt", "optimal", &[(2589, 2)]),
        ],
    )]);

    let plan = &plans[0];
    assert_eq!(plan.skill_ups.len(), 1, "only the orange one");
    assert_eq!(plan.skill_ups[0].recipe, "Linen Belt");
    assert_eq!(
        plan.craft_next().map(|entry| entry.recipe.as_str()),
        Some("Linen Belt")
    );
}

#[test]
fn every_recipe_grey_is_reported_rather_than_read_as_nothing_to_do() {
    // The state that looks like "you are fine" and means the opposite: more
    // crafting cannot help, and what is needed is recipes, not materials.
    let plans = plan_for(vec![character(
        "Tailor",
        20,
        &[(2589, 100)],
        "Tailoring",
        60,
        75,
        vec![recipe("Linen Cloak", "trivial", &[(2589, 2)])],
    )]);

    let plan = &plans[0];
    assert!(plan.out_of_useful_recipes());
    assert!(plan.craft_next().is_none());
    assert!(
        plan.notes.iter().any(|note| note.contains("grey")),
        "got {:?}",
        plan.notes
    );
}

#[test]
fn training_is_due_only_when_both_gates_are_met() {
    // Two gates that fail differently, and telling them apart is the whole
    // value: short on skill means keep crafting, short on level means the
    // profession is not the thing to work on at all.
    assert_eq!(professions::training(60, 75, 20, TBC), Training::NotYet);

    match professions::training(75, 75, 20, TBC) {
        Training::Due { tier } => {
            assert_eq!(tier.name, "Journeyman");
            assert_eq!(tier.cap, 150);
        }
        other => panic!("expected training due, got {other:?}"),
    }

    // At the Apprentice cap but only level 8: the trainer will not sell to
    // them, and sending them is worse than saying nothing.
    match professions::training(75, 75, 8, TBC) {
        Training::ShortOnLevel { tier, needs } => {
            assert_eq!(tier.name, "Journeyman");
            assert_eq!(needs, 10);
        }
        other => panic!("expected a level gate, got {other:?}"),
    }
    assert!(!professions::training(75, 75, 8, TBC).is_actionable());
}

#[test]
fn a_racial_bonus_raises_the_cap_without_skipping_a_training_tier() {
    // Live case: a Blood Elf enchanter sits at 85 because of a racial +10 on
    // an Apprentice's 75. Reading 85 as "past Apprentice" would skip
    // Journeyman, and the character would be told to train something they
    // already have while the thing they need goes unmentioned.
    let tier = professions::next_tier(85, TBC).expect("there is a tier above 85");
    assert_eq!(tier.name, "Journeyman");
    assert_eq!(tier.cap, 150);

    match professions::training(85, 85, 20, TBC) {
        Training::Due { tier } => assert_eq!(tier.name, "Journeyman"),
        other => panic!("expected Journeyman due, got {other:?}"),
    }

    // And the ordinary cap still resolves to the same place.
    assert_eq!(
        professions::next_tier(75, TBC).map(|t| t.name),
        Some("Journeyman")
    );
    assert_eq!(
        professions::next_tier(150, TBC).map(|t| t.name),
        Some("Expert")
    );
    assert_eq!(
        professions::next_tier(225, TBC).map(|t| t.name),
        Some("Artisan")
    );
    assert_eq!(
        professions::next_tier(300, TBC).map(|t| t.name),
        Some("Master")
    );
    assert_eq!(
        professions::next_tier(375, TBC),
        None,
        "nothing above Master"
    );
}

#[test]
fn a_finished_skill_says_nothing_rather_than_inventing_a_tier() {
    assert_eq!(professions::training(375, 375, 70, TBC), Training::Finished);
    assert!(professions::describe(&Training::Finished, "Mining").is_none());
    assert!(professions::describe(&Training::NotYet, "Mining").is_none());
}

#[test]
fn materials_decide_what_can_be_made_and_what_is_short() {
    let plans = plan_for(vec![character(
        "Tailor",
        20,
        &[(2589, 7)],
        "Tailoring",
        60,
        75,
        vec![
            recipe("Linen Belt", "optimal", &[(2589, 2)]),
            recipe("Silk Hood", "optimal", &[(4305, 3)]),
        ],
    )]);

    let plan = &plans[0];
    let belt = plan
        .skill_ups
        .iter()
        .find(|entry| entry.recipe == "Linen Belt")
        .expect("present");
    assert_eq!(belt.can_make, 3, "7 cloth at 2 each");
    assert!(belt.short_of.is_empty());

    let hood = plan
        .skill_ups
        .iter()
        .find(|entry| entry.recipe == "Silk Hood")
        .expect("present");
    assert_eq!(hood.can_make, 0);
    assert_eq!(hood.short_of.len(), 1);
    assert_eq!(hood.short_of[0].1, 3, "all three are missing");

    // Makeable sorts ahead of not, whatever the colour.
    assert_eq!(plan.skill_ups[0].recipe, "Linen Belt");
}

#[test]
fn a_recipe_with_no_captured_reagents_is_unmakeable_not_free() {
    // The mistake this guards is the one that reads best: an empty reagent
    // list costs nothing, so the recipe looks infinitely makeable.
    let mut bare = recipe("Mystery Cloak", "optimal", &[]);
    bare.reagents = None;
    let plans = plan_for(vec![character(
        "Tailor",
        20,
        &[(2589, 100)],
        "Tailoring",
        60,
        75,
        vec![bare],
    )]);

    let plan = &plans[0];
    assert_eq!(plan.skill_ups[0].can_make, 0);
    assert!(plan.craft_next().is_none());
    assert!(
        plan.notes.iter().any(|note| note.contains("no reagents")),
        "got {:?}",
        plan.notes
    );
}

#[test]
fn an_unrated_recipe_is_kept_rather_than_assumed_grey() {
    // A capture that predates the difficulty field, or a client that words it
    // differently. Dropping it would hide a real option; listing it costs one
    // glance and says plainly that it is unrated.
    let mut unrated = recipe("Odd Recipe", "optimal", &[(2589, 1)]);
    unrated.difficulty = None;
    let plans = plan_for(vec![character(
        "Tailor",
        20,
        &[(2589, 10)],
        "Tailoring",
        60,
        75,
        vec![unrated],
    )]);

    assert_eq!(plans[0].skill_ups.len(), 1);
    assert_eq!(plans[0].skill_ups[0].difficulty, Difficulty::Unknown);
    assert_eq!(Difficulty::Unknown.colour(), "unrated");
    assert!(Difficulty::Unknown.can_raise_skill());
    assert!(!Difficulty::Trivial.can_raise_skill());
}

#[test]
fn colour_order_puts_orange_before_yellow_before_green() {
    // The order a player levels in. Orange always raises the skill; green
    // sometimes does. Sorting by anything else would bury the useful one.
    let plans = plan_for(vec![character(
        "Tailor",
        20,
        &[(2589, 100)],
        "Tailoring",
        60,
        75,
        vec![
            recipe("Green One", "easy", &[(2589, 1)]),
            recipe("Orange One", "optimal", &[(2589, 1)]),
            recipe("Yellow One", "medium", &[(2589, 1)]),
        ],
    )]);

    let order: Vec<&str> = plans[0]
        .skill_ups
        .iter()
        .map(|entry| entry.recipe.as_str())
        .collect();
    assert_eq!(order, vec!["Orange One", "Yellow One", "Green One"]);
}

#[test]
fn a_bank_alts_materials_count_and_the_trip_is_flagged() {
    // The reason bank capture was built. The mats are on the mule, the recipe
    // is on the crafter, and the answer is "yes, after a trip" rather than
    // "no".
    let mut vault = character("Vault", 20, &[], "Mining", 1, 75, vec![]);
    vault.recipes = None;
    vault.bank = Some(wow_coach_core::collector::Bank {
        bags: Some(Vec::new()),
        total_slots: Some(28),
        free_slots: Some(20),
        contents: Some(vec![ItemStack {
            item_id: Some(2589),
            count: Some(40),
            name: None,
        }]),
        captured_at: Some(1_000),
    });

    let plans = plan_for(vec![
        character(
            "Tailor",
            20,
            &[],
            "Tailoring",
            60,
            75,
            vec![recipe("Linen Belt", "optimal", &[(2589, 2)])],
        ),
        vault,
    ]);

    let plan = plans
        .iter()
        .find(|plan| plan.character == "Tailor")
        .expect("present");
    let belt = &plan.skill_ups[0];
    assert_eq!(belt.can_make, 20, "40 banked cloth at 2 each");
    assert!(
        belt.needs_a_bank_trip,
        "owned but not to hand is a different answer from owned"
    );
}

#[test]
fn every_tier_gate_is_ordered_and_reachable() {
    // A table whose caps were out of order would make `next_tier` return the
    // wrong one for some ranks, and nothing else would notice.
    let mut previous = 0;
    for rank in [0u16, 74, 75, 149, 150, 224, 225, 299, 300, 374] {
        let tier = professions::next_tier(rank, TBC).expect("a tier above every rank below 375");
        assert!(tier.cap > rank, "{} is not above {rank}", tier.cap);
        assert!(tier.cap >= previous, "caps must not go backwards");
        previous = tier.cap;
        // Every gate is a real requirement, not a placeholder.
        assert!(tier.level > 0);
        assert!(tier.skill < tier.cap);
    }
}

#[test]
fn forever_stops_at_artisan_and_is_not_offered_a_master_trainer() {
    // The bug this test exists for shipped, briefly. The tier table was
    // written from a Burning Crusade source and applied to every client, so a
    // Forever character at the Artisan cap was told to train Master — a tier
    // that does not exist there. Forever is a modern client running
    // vanilla-era content, and its professions stop where vanilla's did, at
    // 300. Sending a player to look for an NPC that was never implemented is
    // the same failure as the level gate, arriving by a different door.
    const FOREVER: Option<&str> = Some("forever");
    const VANILLA: Option<&str> = Some("classic_era");
    const TBC: Option<&str> = Some("tbc_classic");

    assert_eq!(professions::next_tier(300, FOREVER), None);
    assert_eq!(professions::next_tier(300, VANILLA), None);
    assert_eq!(
        professions::next_tier(300, TBC).map(|tier| tier.name),
        Some("Master"),
        "Burning Crusade is where Master came from"
    );

    assert_eq!(
        professions::training(300, 300, 60, FOREVER),
        Training::Finished,
        "at 300 on Forever there is nothing left to train"
    );
    match professions::training(300, 300, 60, TBC) {
        Training::Due { tier } => assert_eq!(tier.name, "Master"),
        other => panic!("expected Master due on TBC, got {other:?}"),
    }

    // Everything below Artisan is identical on all three, so the split must
    // not disturb the common case.
    for flavor in [FOREVER, VANILLA, TBC] {
        assert_eq!(
            professions::next_tier(75, flavor).map(|tier| tier.name),
            Some("Journeyman"),
            "{flavor:?}"
        );
        assert_eq!(
            professions::next_tier(225, flavor).map(|tier| tier.name),
            Some("Artisan"),
            "{flavor:?}"
        );
    }

    // A client this build has never heard of keeps every tier: far likelier to
    // be a later one than an earlier one, and offering a tier too early beats
    // inventing a cap that stops someone short.
    assert_eq!(
        professions::next_tier(300, Some("some_future_client")).map(|tier| tier.name),
        Some("Master")
    );
    assert_eq!(professions::tiers_for(FOREVER).len(), 4);
    assert_eq!(professions::tiers_for(TBC).len(), 5);
}
