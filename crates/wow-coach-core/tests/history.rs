//! Tests for the history store.
//!
//! The behaviour that matters most is that recording is safe to repeat and
//! that a level-up is not read as losing all your XP.

use wow_coach_core::history::{self, History, Snapshot, XpGain};

fn snapshot(name: &str, at: i64, level: u16, xp: u64, max_xp: u64, copper: u64) -> Snapshot {
    Snapshot {
        key: format!("k:{name}"),
        name: name.to_string(),
        captured_at: at,
        level: Some(level),
        xp: Some(xp),
        max_xp: Some(max_xp),
        rested_xp: Some(0),
        money_copper: Some(copper),
        zone: Some("Durotar".to_string()),
        quest_count: Some(3),
        free_bag_slots: Some(20),
        skills: Default::default(),
    }
}

#[test]
fn recording_the_same_capture_twice_changes_nothing() {
    // The tool records automatically, so running it repeatedly between game
    // sessions must be a no-op rather than filling the store with copies.
    let mut history = History::default();
    let first = snapshot("Alt", 1_000, 20, 500, 20_000, 10_000);

    assert!(history.record(first.clone()), "the first is new");
    assert!(!history.record(first.clone()), "the same capture is not");
    assert_eq!(history.len(), 1);

    let later = snapshot("Alt", 2_000, 20, 900, 20_000, 10_000);
    assert!(history.record(later), "a newer capture is new");
    assert_eq!(history.len(), 2);
}

#[test]
fn a_level_up_is_not_read_as_losing_all_your_xp() {
    // Crossing a level resets XP to near zero. Subtracting naively reports a
    // huge loss for the single most positive thing that can happen.
    let before = snapshot("Alt", 1_000, 20, 18_000, 20_000, 0);
    let after = snapshot("Alt", 2_000, 21, 1_500, 22_000, 0);

    let change = history::delta(&before, &after);
    assert_eq!(change.levels_gained, 1);
    // 2000 left in the old level, plus 1500 into the new one.
    assert_eq!(change.xp, XpGain::Exact(3_500));
}

#[test]
fn several_levels_at_once_is_reported_as_a_floor_not_a_figure() {
    // The sizes of the levels in between were never captured, so the true
    // total is unknowable. Saying "at least" beats inventing the rest.
    let before = snapshot("Alt", 1_000, 20, 18_000, 20_000, 0);
    let after = snapshot("Alt", 2_000, 23, 500, 26_000, 0);

    let change = history::delta(&before, &after);
    assert_eq!(change.levels_gained, 3);
    assert_eq!(change.xp, XpGain::AtLeast(2_500));
}

#[test]
fn xp_within_a_level_is_exact() {
    let before = snapshot("Alt", 1_000, 20, 1_000, 20_000, 0);
    let after = snapshot("Alt", 2_000, 20, 4_200, 20_000, 0);
    assert_eq!(history::delta(&before, &after).xp, XpGain::Exact(3_200));
}

#[test]
fn spending_gold_is_reported_as_a_loss_not_as_nothing() {
    let before = snapshot("Alt", 1_000, 20, 0, 20_000, 500_000);
    let after = snapshot("Alt", 2_000, 20, 0, 20_000, 120_000);
    assert_eq!(history::delta(&before, &after).copper, Some(-380_000));
}

#[test]
fn profession_progress_is_tracked_and_new_professions_count_from_zero() {
    let mut before = snapshot("Alt", 1_000, 20, 0, 20_000, 0);
    before.skills.insert("Tailoring".to_string(), 120);
    let mut after = snapshot("Alt", 2_000, 20, 0, 20_000, 0);
    after.skills.insert("Tailoring".to_string(), 150);
    after.skills.insert("Alchemy".to_string(), 40);

    let change = history::delta(&before, &after);
    assert_eq!(change.skills.get("Tailoring"), Some(&30));
    assert_eq!(
        change.skills.get("Alchemy"),
        Some(&40),
        "a newly learned profession counts from zero"
    );
}

#[test]
fn a_character_that_gained_nothing_is_idle() {
    let before = snapshot("Alt", 1_000, 20, 5_000, 20_000, 10_000);
    let after = snapshot("Alt", 9_000, 20, 5_000, 20_000, 10_000);
    assert!(history::delta(&before, &after).is_idle());
}

#[test]
fn stalled_characters_are_the_observation_only_history_can_make() {
    let mut history = History::default();
    // Played.
    history.record(snapshot("Busy", 1_000, 20, 1_000, 20_000, 0));
    history.record(snapshot("Busy", 9_000, 22, 4_000, 24_000, 0));
    // Captured twice, gained nothing.
    history.record(snapshot("Idle", 1_000, 30, 9_000, 38_000, 7_000));
    history.record(snapshot("Idle", 9_000, 30, 9_000, 38_000, 7_000));

    let stalled = history::stalled(&history, 5_000);
    assert_eq!(stalled.len(), 1);
    assert_eq!(stalled[0].name, "Idle");
}

#[test]
fn a_character_captured_only_recently_has_not_stalled() {
    // Two captures an hour apart is a new character, not an abandoned one.
    let mut history = History::default();
    history.record(snapshot("New", 8_000, 5, 100, 2_000, 0));
    history.record(snapshot("New", 9_000, 5, 100, 2_000, 0));
    assert!(history::stalled(&history, 5_000).is_empty());
}

#[test]
fn the_total_spans_the_whole_recorded_history() {
    let mut history = History::default();
    history.record(snapshot("Alt", 1_000, 20, 0, 20_000, 0));
    history.record(snapshot("Alt", 5_000, 20, 5_000, 20_000, 0));
    history.record(snapshot("Alt", 9_000, 21, 2_000, 22_000, 0));

    let total = history::total_delta(&history, "k:Alt").expect("more than one snapshot");
    assert_eq!(total.levels_gained, 1);
    assert_eq!(total.xp, XpGain::Exact(22_000));
    assert_eq!(total.from, 1_000);
    assert_eq!(total.to, 9_000);
}

#[test]
fn a_single_snapshot_yields_no_delta() {
    let mut history = History::default();
    history.record(snapshot("Alt", 1_000, 20, 0, 20_000, 0));
    assert!(history::total_delta(&history, "k:Alt").is_none());
}

#[test]
fn a_damaged_line_costs_one_snapshot_not_the_whole_store() {
    let good = history::to_jsonl_line(&snapshot("Alt", 1_000, 20, 0, 20_000, 0)).unwrap();
    let also_good = history::to_jsonl_line(&snapshot("Alt", 2_000, 21, 5, 22_000, 0)).unwrap();
    let text = format!("{good}\n{{ this is not json\n\n{also_good}\n");

    let (history, problems) = history::parse_jsonl(&text);
    assert_eq!(history.len(), 2, "the readable snapshots survive");
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].contains("line 2"),
        "and it says which line: {problems:?}"
    );
}

#[test]
fn snapshots_round_trip_through_the_store_format() {
    let original = snapshot("Alt", 1_000, 20, 500, 20_000, 9_999);
    let line = history::to_jsonl_line(&original).unwrap();
    let (history, problems) = history::parse_jsonl(&line);
    assert!(problems.is_empty());
    assert_eq!(history.snapshots()[0], original);
}
