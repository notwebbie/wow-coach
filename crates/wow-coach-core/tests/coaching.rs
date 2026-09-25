//! Tests for the ranking engine.
//!
//! These encode the failures of the prototype as much as the rules: it ranked
//! alphabetically while quoting rested XP as the reason, and it presented a
//! winner chosen from a fraction of the roster without saying so.

use std::collections::BTreeMap;

use wow_coach_core::coaching::{self, Rested};
use wow_coach_core::collector::{CharacterRecord, Quest, QuestState, Skill, Talents};
use wow_coach_core::roster::{self, Role, RosterConfig};

fn character(name: &str, level: u16, rested: u64, max_xp: u64) -> CharacterRecord {
    CharacterRecord {
        name: Some(name.to_string()),
        level: Some(level),
        max_xp: Some(max_xp),
        rested_xp: Some(rested),
        is_resting: Some(true),
        game_flavor: Some("tbc_classic".to_string()),
        ..Default::default()
    }
}

/// The cap is 1.5 levels, so this is the rested value that reaches it.
fn cap_for(max_xp: u64) -> u64 {
    max_xp * 3 / 2
}

fn roster_of(records: Vec<CharacterRecord>) -> BTreeMap<String, CharacterRecord> {
    records
        .into_iter()
        .map(|record| (record.name.clone().unwrap(), record))
        .collect()
}

#[test]
fn a_capped_character_outranks_a_fuller_one_that_is_still_filling() {
    // The prototype's exact failure: it would have picked by name. The point
    // is that a smaller rested bar which has STOPPED growing is more urgent
    // than a larger one that is still growing.
    let characters = roster_of(vec![
        character("Zeta", 20, cap_for(20_800), 20_800), // at cap
        character("Alpha", 30, 27_000, 38_800),         // 70%, still filling
    ]);
    let config = RosterConfig::default();
    let entries = roster::roster(&characters, &config);
    let advice = coaching::advise(&entries, true);

    let next = advice.play_next().expect("something should be suggested");
    assert_eq!(
        next.name, "Zeta",
        "the capped character is the one losing rest, even though it is lower level \
         and alphabetically last"
    );
    assert!(matches!(next.rested, Rested::AtCap { .. }));
}

#[test]
fn among_capped_characters_the_higher_level_goes_first() {
    // Rested XP is worth more per point at higher level, so the higher-level
    // capped character is losing more by waiting.
    let characters = roster_of(vec![
        character("Low", 20, cap_for(20_800), 20_800),
        character("High", 40, cap_for(62_400), 62_400),
    ]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);
    assert_eq!(advice.play_next().unwrap().name, "High");
}

#[test]
fn the_reason_contains_the_arithmetic_that_produced_it() {
    // A recommendation that cannot show its numbers is a guess.
    let characters = roster_of(vec![character("Capped", 20, cap_for(20_800), 20_800)]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);

    let reason = advice.play_next().unwrap().reasons.join(" ");
    assert!(reason.contains("31200"), "the rested figure: {reason}");
    assert!(
        reason.contains("stopped accruing"),
        "and what it means: {reason}"
    );
}

#[test]
fn a_character_still_filling_is_described_as_working_not_neglected() {
    let characters = roster_of(vec![character("Filling", 30, 10_000, 38_800)]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);
    let reason = advice.play_next().unwrap().reasons.join(" ");
    assert!(
        reason.contains("still filling"),
        "leaving it parked is the right thing, and the wording should say so: {reason}"
    );
}

#[test]
fn a_character_not_resting_is_told_to_be_parked_not_played() {
    let mut record = character("Standing", 25, 500, 25_000);
    record.is_resting = Some(false);
    let characters = roster_of(vec![record]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);

    assert!(advice.play.is_empty(), "it is not a play suggestion");
    assert_eq!(advice.park.len(), 1);
    assert!(advice.park[0].reasons.join(" ").contains("Park it"));
}

#[test]
fn frictions_break_ties_and_name_what_an_hour_would_fix() {
    let mut busy = character("Busy", 20, cap_for(20_800), 20_800);
    busy.quests = Some(
        (0..24)
            .map(|index| Quest {
                quest_id: Some(index),
                state: Some(if index == 0 {
                    QuestState::Complete
                } else {
                    QuestState::Active
                }),
                ..Default::default()
            })
            .collect(),
    );
    busy.talents = Some(Talents {
        trees: None,
        unspent_points: Some(2),
    });
    let idle = character("Idle", 20, cap_for(20_800), 20_800);

    let characters = roster_of(vec![busy, idle]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);

    let next = advice.play_next().unwrap();
    assert_eq!(
        next.name, "Busy",
        "equally capped, but one session fixes more"
    );

    let said = next
        .frictions
        .iter()
        .map(|friction| friction.summary.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        said.contains("24/25"),
        "quest log pressure with numbers: {said}"
    );
    assert!(said.contains("ready to turn in"), "turn-ins: {said}");
    assert!(said.contains("unspent talent"), "talents: {said}");
}

#[test]
fn a_profession_at_its_cap_is_friction_worth_naming() {
    let mut record = character("Crafter", 20, cap_for(20_800), 20_800);
    record.skills = Some(vec![Skill {
        name: Some("Alchemy".to_string()),
        rank: Some(150),
        max_rank: Some(150),
        skill_id: None,
    }]);
    // The recipe cache is what proves this is a trade skill rather than a
    // weapon skill or a language, which also sit permanently at cap.
    let mut recipes = std::collections::BTreeMap::new();
    recipes.insert(
        "Alchemy".to_string(),
        wow_coach_core::collector::ProfessionRecipes::default(),
    );
    record.recipes = Some(recipes);
    let characters = roster_of(vec![record]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);
    assert!(advice
        .play_next()
        .unwrap()
        .frictions
        .iter()
        .any(|friction| {
            friction.summary.contains("Alchemy") && friction.summary.contains("needs training")
        }));
}

#[test]
fn bank_alts_are_not_play_suggestions_and_are_not_nagged() {
    let characters = roster_of(vec![
        character("Banker", 20, cap_for(20_800), 20_800),
        character("Player", 21, 1_000, 22_000),
    ]);
    let mut config = RosterConfig::default();
    config.set_role("Banker", Role::Bank);

    let entries = roster::roster(&characters, &config);
    let advice = coaching::advise(&entries, true);

    assert_eq!(advice.play.len(), 1);
    assert_eq!(
        advice.play_next().unwrap().name,
        "Player",
        "the capped character is a bank alt and is not a play suggestion"
    );
}

#[test]
fn an_incomplete_roster_qualifies_the_advice() {
    let characters = roster_of(vec![character("Only", 20, cap_for(20_800), 20_800)]);
    let entries = roster::roster(&characters, &RosterConfig::default());

    let complete = coaching::advise(&entries, true);
    assert!(complete.caveats.is_empty());

    let partial = coaching::advise(&entries, false);
    assert!(
        partial
            .caveats
            .iter()
            .any(|caveat| caveat.contains("never been captured")),
        "ranking a fraction of the roster must say so: {:?}",
        partial.caveats
    );
}

#[test]
fn forever_characters_are_not_ranked_on_a_rested_model_we_do_not_have() {
    let mut record = character("Foreverchar", 24, 24_600, 16_400);
    record.game_flavor = Some("forever".to_string());
    let characters = roster_of(vec![record]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);

    let next = advice.play_next().unwrap();
    assert!(matches!(next.rested, Rested::Unknown { .. }));
    assert!(
        next.reasons.join(" ").contains("Well Rested"),
        "it should say why it cannot rank this one: {:?}",
        next.reasons
    );
    assert!(advice
        .caveats
        .iter()
        .any(|caveat| caveat.contains("could not be ranked")));
}

#[test]
fn forever_quest_logs_are_bigger_and_the_threshold_follows() {
    // 24 quests is nearly full on TBC and unremarkable on Forever.
    let mut tbc = character("Tbc", 20, 0, 20_800);
    tbc.quests = Some((0..24).map(|_| Quest::default()).collect());
    let mut forever = character("Forever", 20, 0, 20_800);
    forever.game_flavor = Some("forever".to_string());
    forever.quests = Some((0..24).map(|_| Quest::default()).collect());

    let characters = roster_of(vec![tbc, forever]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);

    let pressured = |name: &str| {
        advice
            .play
            .iter()
            .find(|suggestion| suggestion.name == name)
            .unwrap()
            .frictions
            .iter()
            .any(|friction| friction.summary.contains("quest log"))
    };
    assert!(pressured("Tbc"), "24 of 25 is pressure");
    assert!(!pressured("Forever"), "24 of 40 is not");
}

#[test]
fn an_empty_roster_produces_no_advice_and_does_not_panic() {
    let characters = BTreeMap::new();
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);
    assert!(advice.play_next().is_none());
    assert!(advice.caveats.is_empty());
}

#[test]
fn only_actual_trade_skills_raise_the_training_friction() {
    // The client's skill list also holds talent tabs, weapon skills, armour
    // proficiencies and languages. All sit permanently at cap and none is
    // helped by a trainer. Real data reported six of these as professions.
    use std::collections::BTreeMap;
    use wow_coach_core::collector::ProfessionRecipes;

    let mut record = character("Warlock", 20, cap_for(20_800), 20_800);
    record.skills = Some(vec![
        Skill {
            name: Some("Affliction".into()),
            rank: Some(1),
            max_rank: Some(1),
            skill_id: None,
        },
        Skill {
            name: Some("Language: Orcish".into()),
            rank: Some(300),
            max_rank: Some(300),
            skill_id: None,
        },
        Skill {
            name: Some("Cloth".into()),
            rank: Some(1),
            max_rank: Some(1),
            skill_id: None,
        },
        Skill {
            name: Some("Tailoring".into()),
            rank: Some(150),
            max_rank: Some(150),
            skill_id: None,
        },
    ]);

    // Without recipe evidence, nothing is known to be a trade skill.
    let characters = roster_of(vec![record.clone()]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);
    assert!(
        !advice
            .play_next()
            .unwrap()
            .frictions
            .iter()
            .any(|f| f.summary.contains("cap (")),
        "no skill should be called a profession without evidence: {:?}",
        advice.play_next().unwrap().frictions
    );

    // With Tailoring in the recipe cache, only Tailoring qualifies.
    let mut recipes = BTreeMap::new();
    recipes.insert("Tailoring".to_string(), ProfessionRecipes::default());
    record.recipes = Some(recipes);

    let characters = roster_of(vec![record]);
    let entries = roster::roster(&characters, &RosterConfig::default());
    let advice = coaching::advise(&entries, true);
    let said: Vec<&str> = advice
        .play_next()
        .unwrap()
        .frictions
        .iter()
        .map(|f| f.summary.as_str())
        .collect();
    assert!(
        said.iter().any(|s| s.contains("Tailoring is at its cap")),
        "{said:?}"
    );
    assert!(!said.iter().any(|s| s.contains("Orcish")), "{said:?}");
    assert!(!said.iter().any(|s| s.contains("Affliction")), "{said:?}");
    assert!(!said.iter().any(|s| s.contains("Cloth")), "{said:?}");
}
