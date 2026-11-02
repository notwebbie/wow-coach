//! Tests for the gear plan.
//!
//! The two fixtures are the two cases on purpose: the TBC character is a level
//! 34 mage with no wand and several bare slots, and the Forever one is a level
//! 24 mage carrying a Lesser Magic Wand. One should be told to go shopping and
//! the other should not.

use wow_coach_core::collector::{self, CharacterRecord, EquippedItem};
use wow_coach_core::gear::{self, slot, Standing};

/// The one rung reported for a ladder. Only ever one, by construction.
fn rung<'a>(plan: &'a gear::GearPlan, line: &str) -> &'a gear::MilestoneStatus {
    let found: Vec<&gear::MilestoneStatus> =
        plan.milestones.iter().filter(|m| m.line == line).collect();
    assert!(
        found.len() <= 1,
        "a ladder reports one rung, not {}: {found:#?}",
        found.len()
    );
    found
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no {line} rung in {:#?}", plan.milestones))
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture")
}

fn only_character(name: &str) -> (String, CharacterRecord) {
    let loaded = collector::load(&fixture(name)).expect("the fixture parses");
    let (key, record) = loaded
        .db
        .characters
        .into_iter()
        .next()
        .expect("one character");
    (key, record)
}

fn ungeared() -> CharacterRecord {
    only_character("collector-v3.tbc.lua").1
}

fn geared() -> CharacterRecord {
    only_character("collector-v3.forever.lua").1
}

#[test]
fn a_caster_without_a_wand_is_told_to_get_one() {
    let plan = gear::plan(&ungeared(), "Testhero");

    let wand = rung(&plan, "wand");
    assert_eq!(
        wand.standing,
        Standing::Due,
        "level 34 with an empty ranged slot is overdue: {wand:?}"
    );
    assert_eq!(
        wand.what, "A better wand",
        "and at 34 it is the upgrade rung, not the starter one — telling a \
         player to get both is two instructions for one empty slot"
    );
    assert!(
        wand.how.contains("Enchanting"),
        "and the advice says where to get one: {}",
        wand.how
    );
}

#[test]
fn a_starter_wand_at_level_24_is_an_upgrade_rather_than_a_pass() {
    // The Forever fixture carries a Lesser Magic Wand, item level 14, on a
    // level 24 mage. Having *a* wand is satisfied; having a current one is
    // not, and the ladder is what tells those apart.
    let plan = gear::plan(&geared(), "Trollmage");
    let wand = rung(&plan, "wand");
    assert_eq!(wand.what, "A better wand");
    assert_eq!(wand.standing, Standing::Due, "{wand:?}");
}

#[test]
fn a_wand_that_clears_the_bar_ends_the_ladder() {
    let mut record = geared();
    for item in record
        .equipment
        .as_mut()
        .unwrap()
        .slots
        .as_mut()
        .unwrap()
        .iter_mut()
    {
        if item.is_sub_type("Wands") {
            item.item_level = Some(23);
        }
    }

    let plan = gear::plan(&record, "Trollmage");
    assert_eq!(rung(&plan, "wand").standing, Standing::Met);
    assert!(
        plan.due().next().is_none(),
        "nothing left to do about wands"
    );
}

#[test]
fn the_wand_is_found_by_what_it_is_not_by_which_slot_it_sits_in() {
    // Forever is a modern client running vanilla content, and where it puts a
    // wand is its business. Move the fixture's wand and the verdict must not
    // move with it. Asserted as before-equals-after rather than against a
    // fixed standing, so the test keeps its meaning when the fixture's gear
    // changes — a rule keyed on slot 18 would break here, and would otherwise
    // break silently in the game rather than in this test.
    let before = gear::plan(&geared(), "Trollmage");

    let mut record = geared();
    let kit = record.equipment.as_mut().expect("equipment");
    let slots = kit.slots.as_mut().expect("slots");
    let moved = slots
        .iter_mut()
        .filter(|item| item.is_sub_type("Wands"))
        .map(|item| {
            item.slot = Some(slot::MAIN_HAND);
        })
        .count();
    assert_eq!(moved, 1, "the fixture has exactly one wand to move");

    let after = gear::plan(&record, "Trollmage");
    assert_eq!(
        rung(&before, "wand").standing,
        rung(&after, "wand").standing,
        "a wand is a wand wherever the client hangs it"
    );
}

#[test]
fn an_item_the_client_never_described_is_not_evidence_of_absence() {
    // An unidentified item in the RANGED slot could be the wand, so the honest
    // answer is "cannot tell" rather than "you have no wand" — telling a
    // player to go and buy something they are already carrying is how a tool
    // loses trust.
    let mut record = ungeared();
    let kit = record.equipment.as_mut().expect("equipment");
    kit.slots.as_mut().unwrap().push(EquippedItem {
        slot: Some(slot::RANGED),
        item_id: Some(54321),
        ..EquippedItem::default()
    });

    let plan = gear::plan(&record, "Testhero");
    let wand = rung(&plan, "wand");
    assert!(
        matches!(wand.standing, Standing::Unknowable { .. }),
        "an unidentifiable item blocks the conclusion: {:?}",
        wand.standing
    );
}

#[test]
fn an_unidentified_trinket_cannot_be_a_wand_and_does_not_cloud_the_answer() {
    // The committed TBC fixture wears two trinkets the item cache had not
    // loaded. The first version of this rule let any unidentified item
    // anywhere make every milestone unanswerable, so a mage with a visibly
    // empty ranged slot was told its wand status could not be determined.
    // Whatever those trinkets are, they are not wands: the client put them in
    // trinket slots.
    let record = ungeared();
    assert!(
        record
            .equipment
            .as_ref()
            .unwrap()
            .worn()
            .iter()
            .any(|item| !item.type_is_known()),
        "the fixture still carries the uncached items this test is about"
    );

    let plan = gear::plan(&record, "Testhero");
    let wand = rung(&plan, "wand");
    assert_eq!(
        wand.standing,
        Standing::Due,
        "an unidentified trinket must not cloud a question about weapons: {:?}",
        wand.standing
    );
}

#[test]
fn empty_slots_are_listed_but_the_cosmetic_ones_are_not() {
    let plan = gear::plan(&ungeared(), "Testhero");
    let named: Vec<&str> = plan.empty_slots.iter().map(|s| s.name).collect();

    assert!(
        named.contains(&"Shoulder") && named.contains(&"Legs"),
        "real gaps are reported: {named:?}"
    );
    assert!(
        !named.contains(&"Shirt") && !named.contains(&"Tabard"),
        "a shirt costs no stats, and listing it buries the slots that do: {named:?}"
    );
    assert!(
        !named.contains(&"Head"),
        "the fixture wears a hat, so it is not a gap: {named:?}"
    );
}

#[test]
fn a_two_hander_does_not_leave_the_off_hand_looking_empty() {
    // The TBC mage carries a staff. An off-hand "gap" beside it is advice to
    // go and find something that cannot be equipped.
    let plan = gear::plan(&ungeared(), "Testhero");
    let named: Vec<&str> = plan.empty_slots.iter().map(|s| s.name).collect();
    assert!(
        !named.contains(&"Off hand"),
        "a two-handed weapon occupies the off hand: {named:?}"
    );

    // And with a one-hander it is a real gap again.
    let mut record = ungeared();
    let kit = record.equipment.as_mut().unwrap();
    for item in kit.slots.as_mut().unwrap().iter_mut() {
        if item.slot == Some(slot::MAIN_HAND) {
            item.equip_loc = Some("INVTYPE_WEAPONMAINHAND".to_string());
        }
    }
    let plan = gear::plan(&record, "Testhero");
    assert!(
        plan.empty_slots.iter().any(|s| s.name == "Off hand"),
        "with a one-hander the off hand is genuinely empty"
    );
}

#[test]
fn no_capture_is_reported_as_silence_rather_than_as_being_fully_geared() {
    let mut record = ungeared();
    record.equipment = None;

    let plan = gear::plan(&record, "Testhero");
    assert!(!plan.equipment_known, "nothing has been captured");
    assert!(
        plan.empty_slots.is_empty() && plan.milestones.is_empty(),
        "and nothing is claimed about what is or is not worn"
    );
    assert!(
        plan.caveats.iter().any(|c| c.contains("Log in and out")),
        "the reader is told why it is empty and how to fix it: {:?}",
        plan.caveats
    );
}

#[test]
fn a_character_wearing_nothing_is_distinguished_from_one_never_looked_at() {
    // Equipment captured, nineteen slots asked about, nothing found. That is a
    // real naked character and the opposite conclusion from "nobody looked",
    // from data that looks almost identical. The distinction lives in the
    // counts rather than in the list of holes, because a character with
    // nothing on has nothing conspicuous about any one slot.
    let mut record = ungeared();
    let kit = record.equipment.as_mut().unwrap();
    kit.slots = Some(Vec::new());
    kit.slots_read = Some(19);

    let plan = gear::plan(&record, "Testhero");
    assert!(plan.equipment_known, "the question was asked and answered");
    assert_eq!(plan.slots_filled, 0, "and answered with nothing");
    assert!(
        plan.slots_possible > 10,
        "while still saying how many slots there were to fill: {}",
        plan.slots_possible
    );
}

#[test]
fn a_freshly_created_character_is_not_told_it_is_missing_fifteen_things() {
    // The launch-night case, found by running the plan against four level 1
    // characters: every one reported fifteen empty slots, which is what a new
    // character IS. It was the gear screen's most prominent output and its
    // least useful.
    let mut record = ungeared();
    record.level = Some(1);
    let kit = record.equipment.as_mut().unwrap();
    kit.slots = Some(vec![EquippedItem {
        slot: Some(slot::MAIN_HAND),
        item_id: Some(25),
        name: Some("Worn Shortsword".to_string()),
        item_sub_type: Some("Swords".to_string()),
        equip_loc: Some("INVTYPE_WEAPONMAINHAND".to_string()),
        ..EquippedItem::default()
    }]);

    let plan = gear::plan(&record, "Newbie");
    assert!(
        plan.empty_slots.is_empty(),
        "one item and fourteen holes says only 'you are new': {:?}",
        plan.empty_slots.iter().map(|s| s.name).collect::<Vec<_>>()
    );
    assert_eq!(plan.slots_filled, 1, "the ratio is still carried");
    assert!(plan.slots_possible >= 15);
}

#[test]
fn a_mostly_geared_character_still_hears_about_its_holes() {
    // The other side of the same rule, and the reason the threshold is not a
    // half: the committed fixture is a level 34 with seven of fifteen slots
    // filled, and those bare shoulders are worth saying out loud.
    let plan = gear::plan(&ungeared(), "Testhero");
    assert!(
        !plan.empty_slots.is_empty(),
        "seven filled of {} is a wardrobe with gaps, not a new character",
        plan.slots_possible
    );
    assert!(
        plan.empty_slots.iter().any(|s| s.name == "Shoulder"),
        "{:?}",
        plan.empty_slots.iter().map(|s| s.name).collect::<Vec<_>>()
    );
}

#[test]
fn the_threshold_is_a_third_and_the_counts_always_tell_the_truth() {
    // Walk a character from naked to geared and watch the list switch on. The
    // counts must be right at every step, because a client showing "4 of 16"
    // is relying on them whether or not the list is populated.
    let mut record = ungeared();
    let possible = gear::plan(&record, "Testhero").slots_possible;

    for filled in 0..=possible {
        let kit = record.equipment.as_mut().unwrap();
        // Fill from the top of the reportable list, skipping the main hand so
        // the two-hander rule does not move `slots_possible` mid-walk.
        kit.slots = Some(
            (1..=20u8)
                .filter(|slot| gear::slot_name(*slot).is_some())
                .filter(|slot| {
                    *slot != slot::MAIN_HAND
                        && *slot != slot::SHIRT
                        && *slot != slot::RANGED
                        && *slot != slot::TABARD
                })
                .take(filled)
                .map(|slot| EquippedItem {
                    slot: Some(slot),
                    item_id: Some(1000 + u32::from(slot)),
                    item_sub_type: Some("Cloth".to_string()),
                    ..EquippedItem::default()
                })
                .collect(),
        );

        let plan = gear::plan(&record, "Testhero");
        assert_eq!(plan.slots_filled, filled.min(plan.slots_possible));
        let listed = !plan.empty_slots.is_empty();
        let expected =
            plan.slots_filled * 3 >= plan.slots_possible && plan.slots_filled < plan.slots_possible;
        assert_eq!(
            listed, expected,
            "at {filled} of {} filled, listed={listed}",
            plan.slots_possible
        );
    }
}

#[test]
fn an_empty_list_with_no_arity_is_not_an_answer() {
    // The same empty list, but without `slotsRead` to say the question was
    // asked. This is what an older collector's file looks like, and reading it
    // as "wearing nothing" would strip every character captured before the
    // equipment capture existed.
    let mut record = ungeared();
    let kit = record.equipment.as_mut().unwrap();
    kit.slots = Some(Vec::new());
    kit.slots_read = None;

    let plan = gear::plan(&record, "Testhero");
    assert!(
        !plan.equipment_known,
        "without the arity the empty list says nothing"
    );
}

#[test]
fn a_class_with_no_researched_milestones_says_so() {
    let mut record = ungeared();
    record.class = Some("ROGUE".to_string());

    let plan = gear::plan(&record, "Testhero");
    assert!(
        plan.milestones.is_empty(),
        "nobody has sourced rogue gear yet"
    );
    assert!(
        plan.caveats.iter().any(|c| c.contains("silence")),
        "and the gap in the data is admitted rather than read as 'nothing to do': {:?}",
        plan.caveats
    );
    assert!(
        !plan.empty_slots.is_empty(),
        "while the half that needs no research keeps working"
    );
}

#[test]
fn a_milestone_below_the_level_is_not_yet_rather_than_overdue() {
    let mut record = geared();
    record.level = Some(3);
    let kit = record.equipment.as_mut().unwrap();
    kit.slots = Some(
        kit.worn()
            .iter()
            .filter(|item| !item.is_sub_type("Wands"))
            .cloned()
            .collect(),
    );

    let plan = gear::plan(&record, "Trollmage");
    let wand = rung(&plan, "wand");
    assert_eq!(wand.standing, Standing::NotYet { at_level: 5 });
    assert!(
        plan.due().next().is_none(),
        "and a level 3 character is not nagged about it"
    );
}

#[test]
fn every_milestone_carries_where_it_came_from() {
    // The guard on this table growing careless. A row whose provenance is
    // blank is a row somebody remembered rather than read, and this project
    // has been burned by exactly that before.
    for milestone in gear::MILESTONES {
        assert!(
            !milestone.provenance.source.trim().is_empty(),
            "{} has no source",
            milestone.what
        );
        assert!(
            milestone.provenance.read_on.len() == 10,
            "{} has no ISO date: {}",
            milestone.what,
            milestone.provenance.read_on
        );
        assert!(
            !milestone.how.trim().is_empty(),
            "{} says it matters but not where to get it",
            milestone.what
        );
    }
}

#[test]
fn an_unchecked_milestone_is_flagged_all_the_way_to_the_reader() {
    // The hunter row is carried over from Classic and has not been checked
    // against a Forever source. That has to survive into the output, because a
    // reader cannot weigh a claim whose standing was dropped on the way.
    let mut record = ungeared();
    record.class = Some("HUNTER".to_string());

    let plan = gear::plan(&record, "Testhero");
    let ranged = rung(&plan, "ranged");
    assert!(
        !ranged.checked_for_forever,
        "the flag reaches the client rather than stopping at the table"
    );
}

#[test]
fn an_unidentified_item_does_not_block_a_milestone_already_met() {
    // Unknowable must not outrank Met. A hunter carrying a bow and one mystery
    // trinket has a bow, and saying "cannot tell" there would be a worse answer
    // than the truth sitting right beside it.
    let mut record = geared();
    record.class = Some("HUNTER".to_string());
    let kit = record.equipment.as_mut().unwrap();
    let slots = kit.slots.as_mut().unwrap();
    for item in slots.iter_mut() {
        if item.is_sub_type("Wands") {
            item.item_sub_type = Some("Bows".to_string());
        }
    }
    slots.push(EquippedItem {
        slot: Some(slot::TRINKET_1),
        item_id: Some(12345),
        ..EquippedItem::default()
    });

    let plan = gear::plan(&record, "Trollmage");
    let ranged = rung(&plan, "ranged");
    assert_eq!(
        ranged.standing,
        Standing::Met,
        "the bow is right there: {:?}",
        ranged.standing
    );
}

/// A character of `class` at `level` wearing one piece of `armour`, or nothing
/// if `armour` is None.
fn armoured(class: &str, level: u16, armour: Option<&str>) -> CharacterRecord {
    let mut record = ungeared();
    record.class = Some(class.to_string());
    record.level = Some(level);
    let kit = record.equipment.as_mut().unwrap();
    kit.slots = Some(
        armour
            .map(|sub| {
                vec![EquippedItem {
                    slot: Some(slot::CHEST),
                    item_id: Some(777),
                    item_type: Some("Armor".to_string()),
                    item_sub_type: Some(sub.to_string()),
                    equip_loc: Some("INVTYPE_CHEST".to_string()),
                    ..EquippedItem::default()
                }]
            })
            .unwrap_or_default(),
    );
    record
}

#[test]
fn a_shaman_past_forty_still_in_leather_is_told_about_mail() {
    let plan = gear::plan(&armoured("SHAMAN", 45, Some("Leather")), "Durnan");
    let armour = rung(&plan, "armour");
    assert_eq!(armour.what, "Mail armour");
    assert_eq!(armour.standing, Standing::Due, "{armour:?}");
}

#[test]
fn one_piece_of_the_new_armour_is_enough_to_stop_nagging() {
    // The step change is realising the tier is open at all, not finishing the
    // set. A player wearing one mail piece has noticed; the rest follows from
    // ordinary levelling and does not need a coach.
    let plan = gear::plan(&armoured("SHAMAN", 45, Some("Mail")), "Durnan");
    assert_eq!(rung(&plan, "armour").standing, Standing::Met);
}

#[test]
fn below_forty_the_armour_tier_is_context_rather_than_a_job() {
    let plan = gear::plan(&armoured("SHAMAN", 30, Some("Leather")), "Durnan");
    assert_eq!(
        rung(&plan, "armour").standing,
        Standing::NotYet { at_level: 40 }
    );
    assert!(plan.due().next().is_none(), "nothing to do about it yet");
}

#[test]
fn each_class_is_told_about_the_tier_it_actually_unlocks() {
    // A warrior can wear mail from level 1, so mail is not an event for them —
    // plate is. Getting this backwards would have the tool congratulating a
    // level 45 warrior for the mail they have been wearing since level 1.
    let warrior = gear::plan(&armoured("WARRIOR", 45, Some("Mail")), "Karoh");
    let rung_w = rung(&warrior, "armour");
    assert_eq!(rung_w.what, "Plate armour");
    assert_eq!(
        rung_w.standing,
        Standing::Due,
        "mail does not satisfy a plate wearer past 40: {rung_w:?}"
    );

    let paladin = gear::plan(&armoured("PALADIN", 45, Some("Plate")), "Brannic");
    assert_eq!(rung(&paladin, "armour").standing, Standing::Met);

    // And a mage has no armour tier to unlock at all.
    let mage = gear::plan(&armoured("MAGE", 45, Some("Cloth")), "Saelis");
    assert!(
        mage.milestones.iter().all(|m| m.line != "armour"),
        "cloth is cloth forever: {:?}",
        mage.milestones.iter().map(|m| m.what).collect::<Vec<_>>()
    );
}

#[test]
fn two_ladders_report_two_rungs() {
    // A hunter past 40 has both an armour tier and a ranged weapon to think
    // about. They are separate ladders and both should be heard; the
    // one-rung-per-ladder rule narrows within a line, never across them.
    let plan = gear::plan(&armoured("HUNTER", 45, Some("Leather")), "Kazrin");
    assert_eq!(rung(&plan, "armour").what, "Mail armour");
    assert_eq!(rung(&plan, "ranged").what, "A ranged weapon");
}

#[test]
fn a_shaman_is_no_longer_met_with_silence() {
    // The gap this milestone was added to close: one of the launch four got
    // nothing but the "this is silence, not a clean bill of health" caveat.
    let plan = gear::plan(&armoured("SHAMAN", 1, None), "Durnan");
    assert!(
        !plan.milestones.is_empty(),
        "a shaman has something to hear about"
    );
    assert!(
        !plan.caveats.iter().any(|c| c.contains("silence")),
        "and no longer the empty-table caveat: {:?}",
        plan.caveats
    );
}
