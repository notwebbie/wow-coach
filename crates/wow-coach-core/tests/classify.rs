//! Tests for role suggestions.
//!
//! The rule under test throughout: inference proposes, the player disposes.
//! Nothing here may overrule a decision already made, and a guess must never
//! be dressed up as an observation.

use std::collections::BTreeMap;

use wow_coach_core::classify::{self, Confidence, IDLE_SECONDS};
use wow_coach_core::collector::{CharacterRecord, Quest};
use wow_coach_core::history::{History, Snapshot};
use wow_coach_core::roster::{self, Role, RosterConfig};

const NOW: i64 = 1_800_000_000;

fn record(name: &str, level: u16, quests: usize) -> CharacterRecord {
    CharacterRecord {
        name: Some(name.to_string()),
        level: Some(level),
        max_xp: Some(20_000),
        xp: Some(100),
        rested_xp: Some(0),
        is_resting: Some(true),
        captured_at: Some(NOW),
        game_flavor: Some("tbc_classic".to_string()),
        quests: Some((0..quests).map(|_| Quest::default()).collect()),
        ..Default::default()
    }
}

fn characters(records: Vec<CharacterRecord>) -> BTreeMap<String, CharacterRecord> {
    records
        .into_iter()
        .map(|record| (record.name.clone().unwrap(), record))
        .collect()
}

fn snapshot(key: &str, at: i64, level: u16, xp: u64) -> Snapshot {
    Snapshot {
        key: key.to_string(),
        name: key.to_string(),
        captured_at: at,
        level: Some(level),
        xp: Some(xp),
        max_xp: Some(20_000),
        rested_xp: Some(0),
        money_copper: Some(1_000),
        zone: None,
        quest_count: Some(0),
        free_bag_slots: Some(50),
        skills: Default::default(),
    }
}

/// Two captures a month apart with nothing gained in between.
fn idle_history(key: &str) -> History {
    let mut history = History::default();
    history.record(snapshot(key, NOW - 30 * 86_400, 40, 5_000));
    history.record(snapshot(key, NOW, 40, 5_000));
    history
}

#[test]
fn a_quiet_low_level_character_with_no_quests_is_suggested_as_a_bank_alt() {
    let characters = characters(vec![record("Vault", 1, 0)]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let mut history = History::default();
    history.record(snapshot("Vault", NOW - 30 * 86_400, 1, 0));
    history.record(snapshot("Vault", NOW, 1, 0));

    let suggestion = classify::suggest(&entries[0], &history, NOW).expect("should suggest");
    assert_eq!(suggestion.suggested, Role::Bank);
    assert_eq!(suggestion.confidence, Confidence::Observed);
    assert!(suggestion
        .evidence
        .iter()
        .any(|line| line.contains("30 days")));
    assert!(suggestion
        .evidence
        .iter()
        .any(|line| line.contains("empty quest log")));
}

#[test]
fn a_quiet_high_level_character_is_parked_not_banked() {
    // Somebody set this down. It is not storage; it is a character on hold.
    let characters = characters(vec![record("Retired", 40, 6)]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let suggestion =
        classify::suggest(&entries[0], &idle_history("Retired"), NOW).expect("should suggest");
    assert_eq!(suggestion.suggested, Role::Parked);
    assert_eq!(suggestion.confidence, Confidence::Observed);
}

#[test]
fn a_low_level_character_with_no_history_is_only_a_possibility() {
    // It could equally be a character being levelled right now. Worth asking
    // about, not worth assuming.
    let characters = characters(vec![record("Fresh", 2, 0)]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let suggestion =
        classify::suggest(&entries[0], &History::default(), NOW).expect("should suggest");
    assert_eq!(suggestion.suggested, Role::Bank);
    assert_eq!(
        suggestion.confidence,
        Confidence::Possible,
        "nothing confirms it, so it must not claim to be observed"
    );
}

#[test]
fn an_active_character_gets_no_suggestion() {
    let characters = characters(vec![record("Main", 35, 12)]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    assert!(classify::suggest(&entries[0], &History::default(), NOW).is_none());
}

#[test]
fn a_short_quiet_spell_is_not_evidence_of_anything() {
    // A quiet week is a quiet week, not a decision.
    let characters = characters(vec![record("Busy", 40, 8)]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let mut history = History::default();
    history.record(snapshot("Busy", NOW - (IDLE_SECONDS - 86_400), 40, 5_000));
    history.record(snapshot("Busy", NOW, 40, 5_000));
    assert!(classify::suggest(&entries[0], &history, NOW).is_none());
}

#[test]
fn a_levelling_character_is_not_called_idle() {
    let characters = characters(vec![record("Levelling", 40, 8)]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let mut history = History::default();
    history.record(snapshot("Levelling", NOW - 30 * 86_400, 38, 1_000));
    history.record(snapshot("Levelling", NOW, 40, 9_000));
    assert!(classify::suggest(&entries[0], &history, NOW).is_none());
}

#[test]
fn a_decision_already_made_is_never_re_proposed() {
    // Re-asking about something the player settled is how a helpful tool
    // becomes a nagging one.
    let characters = characters(vec![record("Vault", 1, 0)]);
    let mut config = RosterConfig::default();
    config.set_role("Vault", Role::Active);

    let entries = roster::roster(&characters, &config);
    let suggestions = classify::suggest_all(&entries, &config, &idle_history("Vault"), NOW);
    assert!(
        suggestions.is_empty(),
        "an explicit choice stands, whatever the evidence says: {suggestions:?}"
    );
}

#[test]
fn a_suggestion_matching_the_current_role_is_not_raised() {
    let characters = characters(vec![record("Vault", 1, 0)]);
    let mut config = RosterConfig::default();
    config.set_role("Vault", Role::Bank);
    let entries = roster::roster(&characters, &config);
    assert!(classify::suggest(&entries[0], &idle_history("Vault"), NOW).is_none());
}

#[test]
fn observed_suggestions_are_listed_before_guesses() {
    let characters = characters(vec![record("Guess", 2, 0), record("Known", 1, 0)]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let mut history = History::default();
    history.record(snapshot("Known", NOW - 30 * 86_400, 1, 0));
    history.record(snapshot("Known", NOW, 1, 0));

    let suggestions = classify::suggest_all(&entries, &RosterConfig::default(), &history, NOW);
    assert_eq!(suggestions.len(), 2);
    assert_eq!(suggestions[0].name, "Known");
    assert_eq!(suggestions[0].confidence, Confidence::Observed);
    assert_eq!(suggestions[1].confidence, Confidence::Possible);
}

#[test]
fn every_suggestion_carries_its_evidence() {
    let characters = characters(vec![record("Vault", 1, 0)]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let suggestions = classify::suggest_all(
        &entries,
        &RosterConfig::default(),
        &idle_history("Vault"),
        NOW,
    );
    assert!(
        !suggestions[0].evidence.is_empty(),
        "a bare guess is worse than none"
    );
}

#[test]
fn clearing_a_role_is_not_the_same_as_setting_it_active() {
    // Setting a character back to `active` still counts as a decision, so
    // suggestions stop for it forever. Undoing has to remove the entry.
    let characters = characters(vec![record("Vault", 1, 0)]);
    let history = idle_history("Vault");

    let mut config = RosterConfig::default();
    config.set_role("Vault", Role::Bank);
    assert!(config.is_classified("Vault"));

    // "Undo" by setting active: still classified, still silent.
    config.set_role("Vault", Role::Active);
    let entries = roster::roster(&characters, &config);
    assert!(
        classify::suggest_all(&entries, &config, &history, NOW).is_empty(),
        "an explicit active is a decision and must stay silent"
    );

    // Properly undone: unclassified, and open to suggestions again.
    assert!(config.clear_role("Vault"));
    assert!(!config.is_classified("Vault"));
    let entries = roster::roster(&characters, &config);
    assert_eq!(
        classify::suggest_all(&entries, &config, &history, NOW).len(),
        1,
        "clearing should make it suggestible again"
    );

    assert!(!config.clear_role("Vault"), "clearing twice is harmless");
}
