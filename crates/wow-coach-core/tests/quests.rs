//! Tests for the quest planner.
//!
//! The rule under test throughout, from the Phase 3 brief: a line earns its
//! place only if it names something to do. An above-level quest is not a fault
//! — it is a quest you have not got to yet — and it only becomes advice when
//! the log is too full to take anything new.

use wow_coach_core::collector::{CharacterRecord, Quest, QuestState};
use wow_coach_core::quests::{self, Standing};

fn quest(title: &str, level: u16, zone: &str, state: QuestState) -> Quest {
    Quest {
        quest_id: Some(1000 + level as u32),
        title: Some(title.to_string()),
        level: Some(level),
        header: Some(zone.to_string()),
        state: Some(state),
    }
}

fn character(level: u16, quests: Vec<Quest>) -> CharacterRecord {
    CharacterRecord {
        name: Some("Drek".to_string()),
        level: Some(level),
        game_flavor: Some("tbc_classic".to_string()),
        quests_complete: Some(true),
        quests: Some(quests),
        ..Default::default()
    }
}

#[test]
fn the_zone_that_clears_the_most_comes_first() {
    // A quest log is a route, not a list. Ten quests in one zone is one trip;
    // the same ten spread over five zones is five.
    let record = character(
        20,
        vec![
            quest("A", 19, "The Barrens", QuestState::Active),
            quest("B", 20, "The Barrens", QuestState::Active),
            quest("C", 20, "The Barrens", QuestState::Active),
            quest("D", 21, "Stonetalon Mountains", QuestState::Active),
            quest("E", 19, "Stonetalon Mountains", QuestState::Active),
        ],
    );
    let plan = quests::plan(&record, "key");

    assert_eq!(plan.best_zone().unwrap().zone, "The Barrens");
    assert_eq!(plan.best_zone().unwrap().worth_a_trip(), 3);
    assert_eq!(plan.clusters[1].zone, "Stonetalon Mountains");
}

#[test]
fn finished_quests_are_surfaced_wherever_they_are() {
    // The cheapest XP on the whole list: already earned, and each one frees a
    // slot when handed in.
    let record = character(
        31,
        vec![
            quest(
                "Salt Flat Venom",
                30,
                "Thousand Needles",
                QuestState::Complete,
            ),
            quest(
                "Hemet Nesingwary Jr.",
                31,
                "Thousand Needles",
                QuestState::Complete,
            ),
            quest("Frostmaw", 37, "Thousand Needles", QuestState::Active),
        ],
    );
    let plan = quests::plan(&record, "key");

    assert_eq!(plan.turn_ins.len(), 2);
    assert!(plan.turn_ins.iter().any(|q| q.title == "Salt Flat Venom"));
    assert_eq!(
        plan.clusters[0].complete.len(),
        2,
        "and they also count towards the zone being worth visiting"
    );
}

#[test]
fn an_above_level_quest_is_not_a_fault_on_a_roomy_log() {
    // The prototype reported "N orange/red active quests" on every character,
    // which presents a quest you have not got to yet as a problem.
    let record = character(
        20,
        vec![
            quest(
                "Melor Sends Word",
                30,
                "Thousand Needles",
                QuestState::Active,
            ),
            quest("Ordinary", 20, "The Barrens", QuestState::Active),
        ],
    );
    let plan = quests::plan(&record, "key");

    assert!(!plan.log.is_tight());
    assert!(
        plan.drop_candidates.is_empty(),
        "nothing to say about it while there is room"
    );
}

#[test]
fn a_full_log_turns_the_same_quest_into_an_answer() {
    // Same quest, different situation: now it is what you drop.
    let mut entries = vec![quest(
        "Melor Sends Word",
        30,
        "Thousand Needles",
        QuestState::Active,
    )];
    for index in 0..24 {
        entries.push(quest(
            &format!("Filler {index}"),
            20,
            "The Barrens",
            QuestState::Active,
        ));
    }
    let record = character(20, entries);
    let plan = quests::plan(&record, "key");

    assert!(plan.log.is_tight(), "25 of 25");
    assert_eq!(plan.drop_candidates.len(), 1);
    assert_eq!(plan.drop_candidates[0].title, "Melor Sends Word");
    assert!(
        plan.caveats
            .iter()
            .any(|caveat| caveat.contains("has to be picked up again")),
        "and dropping is framed as a cost: {:?}",
        plan.caveats
    );
}

#[test]
fn a_finished_quest_is_never_offered_up_for_dropping() {
    // Dropping a quest that is already done throws away work.
    let mut entries = vec![quest("Done", 12, "The Barrens", QuestState::Complete)];
    for index in 0..24 {
        entries.push(quest(
            &format!("Filler {index}"),
            30,
            "Desolace",
            QuestState::Active,
        ));
    }
    let record = character(20, entries);
    let plan = quests::plan(&record, "key");

    assert!(plan.log.is_tight());
    assert!(
        !plan.drop_candidates.iter().any(|q| q.title == "Done"),
        "a completed quest is XP in hand, not clutter"
    );
    assert!(
        plan.caveats
            .iter()
            .any(|caveat| caveat.contains("frees that many slots")),
        "and handing it in is offered as the way out: {:?}",
        plan.caveats
    );
}

#[test]
fn out_of_reach_goes_before_merely_unrewarding() {
    // If something must go, the one you cannot do beats the one that pays
    // little but is still finishable on the way past.
    let mut entries = vec![
        quest("Way Too High", 40, "Desolace", QuestState::Active),
        quest("Ancient History", 5, "Durotar", QuestState::Active),
    ];
    for index in 0..23 {
        entries.push(quest(
            &format!("Filler {index}"),
            20,
            "The Barrens",
            QuestState::Active,
        ));
    }
    let record = character(20, entries);
    let plan = quests::plan(&record, "key");

    assert_eq!(plan.drop_candidates[0].title, "Way Too High");
    assert_eq!(plan.drop_candidates[1].title, "Ancient History");
}

#[test]
fn standings_use_the_documented_bands() {
    let record = character(
        20,
        vec![
            quest("Just Over", 24, "Zone", QuestState::Active),
            quest("Well Over", 25, "Zone", QuestState::Active),
            quest("Old But Payable", 13, "Zone", QuestState::Active),
            quest("Ancient", 12, "Zone", QuestState::Active),
        ],
    );
    let plan = quests::plan(&record, "key");
    let cluster = &plan.clusters[0];

    let ready: Vec<&str> = cluster.ready.iter().map(|q| q.title.as_str()).collect();
    assert!(
        ready.contains(&"Just Over"),
        "four above is still within reach"
    );
    assert!(
        ready.contains(&"Old But Payable"),
        "seven below still pays enough to be worth finishing"
    );
    assert_eq!(cluster.too_high.len(), 1);
    assert_eq!(cluster.too_high[0].title, "Well Over");
    assert_eq!(cluster.trivial.len(), 1);
    assert_eq!(cluster.trivial[0].title, "Ancient");
}

#[test]
fn a_quest_with_no_level_is_not_judged() {
    let mut bare = quest("Mystery", 1, "Zone", QuestState::Active);
    bare.level = None;
    let record = character(20, vec![bare]);
    let plan = quests::plan(&record, "key");

    assert_eq!(plan.clusters[0].ready.len(), 1, "unjudged is not unlisted");
    assert_eq!(plan.clusters[0].ready[0].standing, Standing::Unknown);
    assert!(plan.clusters[0].trivial.is_empty());
    assert!(plan.clusters[0].too_high.is_empty());
}

#[test]
fn a_partly_captured_log_says_so() {
    // A collapsed header hides quests from enumeration, so the plan is built
    // on less than the whole log and must not pretend otherwise.
    let mut record = character(20, vec![quest("A", 20, "Zone", QuestState::Active)]);
    record.quests_complete = Some(false);
    let plan = quests::plan(&record, "key");

    assert!(
        plan.caveats
            .iter()
            .any(|caveat| caveat.contains("collapsed header")),
        "{:?}",
        plan.caveats
    );
}

#[test]
fn forever_gets_its_larger_log() {
    // The cap differs by client, and a plan that assumed 25 would call a
    // roomy Forever log full.
    let mut entries = Vec::new();
    for index in 0..25 {
        entries.push(quest(
            &format!("Filler {index}"),
            20,
            "Durotar",
            QuestState::Active,
        ));
    }
    let mut record = character(20, entries);
    record.game_flavor = Some("forever".to_string());
    let plan = quests::plan(&record, "key");

    assert_eq!(plan.log.capacity, 40);
    assert!(!plan.log.is_tight(), "25 of 40 is not tight");
    assert!(plan.drop_candidates.is_empty());
}

#[test]
fn a_zone_full_of_quests_you_cannot_do_is_not_worth_a_trip() {
    // The ranking is "how much would visiting here actually clear", not "how
    // many quests are filed under this heading". A zone holding five quests
    // ten levels above you clears nothing, and sending somebody there because
    // the count looked big is the exact mistake the count exists to avoid.
    let record = character(
        20,
        vec![
            quest("High 1", 35, "Desolace", QuestState::Active),
            quest("High 2", 36, "Desolace", QuestState::Active),
            quest("High 3", 37, "Desolace", QuestState::Active),
            quest("High 4", 38, "Desolace", QuestState::Active),
            quest("High 5", 39, "Desolace", QuestState::Active),
            quest("Doable", 20, "The Barrens", QuestState::Active),
        ],
    );
    let plan = quests::plan(&record, "key");

    let desolace = plan
        .clusters
        .iter()
        .find(|cluster| cluster.zone == "Desolace")
        .expect("still listed");
    assert_eq!(desolace.total(), 5, "the quests are still there");
    assert_eq!(
        desolace.worth_a_trip(),
        0,
        "but none of them would be cleared by going"
    );
    assert_eq!(
        plan.best_zone().map(|cluster| cluster.zone.as_str()),
        Some("The Barrens"),
        "so the one doable quest outranks five you cannot touch"
    );
}

#[test]
fn a_zone_of_only_trivial_quests_does_not_outrank_a_real_one() {
    let record = character(
        30,
        vec![
            quest("Old 1", 10, "Durotar", QuestState::Active),
            quest("Old 2", 11, "Durotar", QuestState::Active),
            quest("Old 3", 12, "Durotar", QuestState::Active),
            quest("Current", 30, "Arathi Highlands", QuestState::Active),
        ],
    );
    let plan = quests::plan(&record, "key");

    assert_eq!(
        plan.best_zone().map(|cluster| cluster.zone.as_str()),
        Some("Arathi Highlands")
    );
}
