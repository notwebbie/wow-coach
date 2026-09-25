//! Tests for the collector reader, run against the same fixtures the addon's
//! own test harnesses generate. Those fixtures are regenerated and diffed in
//! CI, so if the collector changes what it writes and this reader is not
//! updated, one side or the other fails.

use std::collections::BTreeMap;
use std::path::PathBuf;

use wow_coach_core::collector::{self, LoadError, QuestState};
use wow_coach_core::lua;
use wow_coach_core::roster::{self, Role, RosterConfig};

fn fixture(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "fixtures", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("could not read fixture {}: {error}", path.display());
    })
}

#[test]
fn reads_the_tbc_fixture() {
    let loaded = collector::load(&fixture("collector-v2.tbc.lua")).expect("fixture should load");
    assert_eq!(loaded.db.schema_version, Some(2));
    assert_eq!(loaded.db.characters.len(), 1);

    let record = loaded.db.characters.values().next().unwrap();
    assert_eq!(record.name.as_deref(), Some("Testhero"));
    assert_eq!(record.game_flavor.as_deref(), Some("tbc_classic"));
    assert_eq!(record.interface_version, Some(20506));
    assert_eq!(record.level, Some(34));
    assert_eq!(record.class.as_deref(), Some("MAGE"));
    assert_eq!(record.is_resting, Some(true));

    let skills = record.skills.as_ref().expect("skills captured");
    let alchemy = skills
        .iter()
        .find(|skill| skill.name.as_deref() == Some("Alchemy"))
        .expect("Alchemy present");
    assert_eq!(alchemy.rank, Some(225));
    assert_eq!(alchemy.max_rank, Some(300));
    // The Classic clients supply no stable skill id; matching is by name.
    assert_eq!(alchemy.skill_id, None);

    let quests = record.quests.as_ref().expect("quests captured");
    assert_eq!(quests.len(), 2);
    assert_eq!(quests[0].quest_id, Some(9440));
    assert_eq!(quests[0].state, Some(QuestState::Complete));
    assert_eq!(quests[1].state, Some(QuestState::Failed));

    let inventory = record.inventory.as_ref().expect("inventory captured");
    assert_eq!(inventory.total_slots, Some(22));
    let contents = inventory.contents.as_ref().unwrap();
    let linen = contents
        .iter()
        .find(|stack| stack.item_id == Some(2589))
        .expect("summed stack present");
    assert_eq!(linen.count, Some(32));

    // No rulesets and no pet on this client family.
    assert!(record.ruleset.is_none());
    assert!(record.pet.is_none());
}

#[test]
fn reads_the_forever_fixture() {
    let loaded =
        collector::load(&fixture("collector-v2.forever.lua")).expect("fixture should load");
    let record = loaded.db.characters.values().next().unwrap();

    assert_eq!(record.game_flavor.as_deref(), Some("forever"));
    assert_eq!(record.realm.as_deref(), Some("Classic Beta PvE"));

    // Forever supplies a stable skill id where the Classic clients do not.
    let skills = record.skills.as_ref().unwrap();
    let alchemy = skills
        .iter()
        .find(|skill| skill.name.as_deref() == Some("Alchemy"))
        .unwrap();
    assert_eq!(alchemy.skill_id, Some(171));
    assert_eq!(alchemy.rank, Some(225));

    // Normal ruleset: every flag known and false. That is data, not absence.
    let ruleset = record.ruleset.expect("ruleset captured on Forever");
    assert_eq!(ruleset.hardcore, Some(false));
    assert_eq!(ruleset.pvp, Some(false));
    assert!(ruleset.is_normal());

    let pet = record.pet.as_ref().expect("pet captured");
    assert_eq!(pet.name.as_deref(), Some("Snarlfang"));
    assert_eq!(pet.training_points_spent, Some(40));

    let talents = record.talents.as_ref().unwrap();
    let trees = talents.trees.as_ref().unwrap();
    let fire = trees
        .iter()
        .find(|tree| tree.name.as_deref() == Some("Fire"))
        .unwrap();
    assert_eq!(fire.points_spent, Some(21));
    assert_eq!(talents.unspent_points, Some(3));

    let recipes = record.recipes.as_ref().unwrap();
    let alchemy = recipes.get("Alchemy").expect("Alchemy recipes cached");
    assert_eq!(alchemy.rank, Some(225));
    assert_eq!(alchemy.recipes.as_ref().unwrap().len(), 2);
}

#[test]
fn a_ruleset_with_an_unknown_flag_is_not_called_normal() {
    let partly_known = collector::Ruleset {
        hardcore: Some(false),
        pvp: None,
        rp: Some(false),
        self_found: None,
    };
    assert!(
        !partly_known.is_normal(),
        "an unknown flag must not be read as absence of that ruleset"
    );
}

#[test]
fn the_rested_cap_is_known_only_where_the_model_holds() {
    let loaded = collector::load(&fixture("collector-v2.tbc.lua")).unwrap();
    let tbc = loaded.db.characters.values().next().unwrap();
    assert!(tbc.rested_cap_is_known());
    // 7800 max XP, so 1.5 levels is 11700; 900 rested is a little under 8%.
    assert_eq!(tbc.rested_cap(), Some(11_700));
    let fraction = tbc.rested_fraction().unwrap();
    assert!((fraction - 900.0 / 11_700.0).abs() < 1e-9);

    let loaded = collector::load(&fixture("collector-v2.forever.lua")).unwrap();
    let forever = loaded.db.characters.values().next().unwrap();
    assert!(
        !forever.rested_cap_is_known(),
        "the Well Rested perk changes the cap and the rate, and neither is exposed"
    );
}

#[test]
fn every_field_may_be_absent() {
    let minimal = r#"WoWCoachCollectorDB = { ["characters"] = { ["k"] = { } } }"#;
    let loaded = collector::load(minimal).expect("a record with nothing in it is still a record");
    let record = loaded.db.characters.get("k").unwrap();
    assert!(record.level.is_none());
    assert!(record.skills.is_none());
    assert_eq!(record.display_name("k"), "k", "falls back to the key");
    assert!(loaded
        .notices
        .iter()
        .any(|notice| notice.contains("no schema version")));
}

#[test]
fn a_newer_schema_is_read_not_rejected() {
    let newer = r#"
        WoWCoachCollectorDB = {
            ["schemaVersion"] = 99,
            ["characters"] = {
                ["k"] = { ["name"] = "Future", ["level"] = 60, ["somethingNew"] = 1 },
            },
        }
    "#;
    let loaded = collector::load(newer).expect("a player who updates the addon first still counts");
    assert_eq!(
        loaded.db.characters.get("k").unwrap().name.as_deref(),
        Some("Future")
    );
    assert!(loaded
        .notices
        .iter()
        .any(|notice| notice.contains("schema v99")));
}

#[test]
fn partial_captures_are_surfaced_not_swallowed() {
    let partial = r#"
        WoWCoachCollectorDB = {
            ["schemaVersion"] = 2,
            ["characters"] = {
                ["k"] = { ["name"] = "Half", ["skillsComplete"] = false,
                          ["questsComplete"] = false },
            },
        }
    "#;
    let loaded = collector::load(partial).unwrap();
    assert!(loaded
        .notices
        .iter()
        .any(|notice| notice.contains("skills were only partly captured")));
    assert!(loaded
        .notices
        .iter()
        .any(|notice| notice.contains("quests were only partly captured")));
}

#[test]
fn rejects_anything_outside_the_subset() {
    // The whole point is that this is not an interpreter.
    for (source, description) in [
        (r#"X = os.time()"#, "a function call"),
        (r#"X = { ["a"] = nil }"#, "a stored nil"),
        (r#"X = { ["a"] = 1 + 2 }"#, "an expression"),
        (r#"X = { ["a"] = 1 "#, "an unterminated table"),
        (r#"X = { [{}] = 1 }"#, "a table used as a key"),
        (r#"X = "unterminated"#, "an unterminated string"),
    ] {
        assert!(
            collector::load(source).is_err() || lua::parse_saved_variables(source).is_err(),
            "{description} should be rejected"
        );
    }
}

#[test]
fn reports_where_the_problem_is() {
    let broken = "WoWCoachCollectorDB = {\n  [\"a\"] = 1,\n  [\"b\"] = @,\n}\n";
    match collector::load(broken) {
        Err(LoadError::Syntax(error)) => {
            assert_eq!(error.line, 3, "the error should name the offending line");
            assert!(error.message.contains('@'));
        }
        other => panic!("expected a syntax error, got {other:?}"),
    }
}

#[test]
fn a_file_without_the_addon_global_says_so() {
    let wrong = r#"SomeOtherAddonDB = { ["a"] = 1 }"#;
    match collector::load(wrong) {
        Err(LoadError::MissingGlobal(name)) => assert_eq!(name, "WoWCoachCollectorDB"),
        other => panic!("expected a missing-global error, got {other:?}"),
    }
}

#[test]
fn array_entries_must_be_in_order() {
    // WoW's writer always emits arrays in order. Anything else came from
    // somewhere else, and quietly reordering it would hide that.
    let shuffled = r#"X = { [2] = "b", [1] = "a" }"#;
    assert!(lua::parse_saved_variables(shuffled).is_err());
}

#[test]
fn parses_the_strings_wow_actually_writes() {
    let source = r#"X = { ["quoted"] = "say \"hi\"", ["path"] = "a\\b",
                          ["newline"] = "one\ntwo", ["byte"] = "\65\66" }"#;
    let globals = lua::parse_saved_variables(source).expect("should parse");
    let table = globals["X"].as_table().unwrap();
    assert_eq!(table.get("quoted").unwrap().as_str(), Some("say \"hi\""));
    assert_eq!(table.get("path").unwrap().as_str(), Some("a\\b"));
    assert_eq!(table.get("newline").unwrap().as_str(), Some("one\ntwo"));
    assert_eq!(table.get("byte").unwrap().as_str(), Some("AB"));
}

#[test]
fn handles_non_ascii_names() {
    // Realm and character names are not all ASCII.
    let source = r#"X = { ["name"] = "Sylvanas Windläufer" }"#;
    let globals = lua::parse_saved_variables(source).expect("should parse");
    let table = globals["X"].as_table().unwrap();
    assert_eq!(
        table.get("name").unwrap().as_str(),
        Some("Sylvanas Windläufer")
    );
}

#[test]
fn a_bank_alt_leaves_the_rotation_but_keeps_its_economy() {
    let loaded = collector::load(&fixture("collector-v2.tbc.lua")).unwrap();
    let key = loaded.db.characters.keys().next().unwrap().clone();

    let mut config = RosterConfig::default();
    config.set_role(key.clone(), Role::Bank);

    let entries = roster::roster(&loaded.db.characters, &config);
    let entry = entries.iter().find(|entry| entry.key == key).unwrap();

    assert_eq!(entry.role, Role::Bank);
    assert!(
        !entry.role.in_play_rotation(),
        "a bank alt is not a play suggestion"
    );
    assert!(
        entry.role.in_economy(),
        "but its professions, bags and gold still count — that is what it is for"
    );
    assert!(
        !entry.role.wants_reminders(),
        "and it should not be nagged about talent points"
    );
}

#[test]
fn roles_default_to_active_and_resolve_by_name() {
    let loaded = collector::load(&fixture("collector-v2.tbc.lua")).unwrap();
    let config = RosterConfig::default();
    let key = loaded.db.characters.keys().next().unwrap();
    assert_eq!(config.role_for(key), Role::Active);

    // Players type names, not storage keys, and not consistently capitalised.
    let resolved = RosterConfig::resolve(&loaded.db.characters, "testhero");
    assert_eq!(resolved, Some(key.as_str()));
    assert_eq!(RosterConfig::resolve(&loaded.db.characters, "nobody"), None);
}

#[test]
fn an_empty_character_list_is_not_an_error() {
    let empty = r#"WoWCoachCollectorDB = { ["schemaVersion"] = 2, ["characters"] = { } }"#;
    let loaded = collector::load(empty).expect("a fresh install has no characters yet");
    assert!(loaded.db.characters.is_empty());
    let entries = roster::roster(&loaded.db.characters, &RosterConfig::default());
    assert!(entries.is_empty());
}

#[test]
fn round_trips_through_serde() {
    let loaded = collector::load(&fixture("collector-v2.forever.lua")).unwrap();
    let json = serde_json::to_string(&loaded.db).unwrap();
    let back: collector::CollectorDb = serde_json::from_str(&json).unwrap();
    assert_eq!(back, loaded.db);
}

#[test]
fn keys_stay_distinct_across_flavors() {
    // The same character name on two clients must not collide, which is what
    // the length-prefixed key is for.
    let both = r#"
        WoWCoachCollectorDB = {
            ["characters"] = {
                ["11:tbc_classic|5:Realm|4:Same"] = { ["name"] = "Same", ["level"] = 30 },
                ["7:forever|5:Realm|4:Same"] = { ["name"] = "Same", ["level"] = 12 },
            },
        }
    "#;
    let loaded = collector::load(both).unwrap();
    assert_eq!(loaded.db.characters.len(), 2);
    let levels: BTreeMap<_, _> = loaded
        .db
        .characters
        .iter()
        .map(|(key, record)| (key.clone(), record.level))
        .collect();
    assert_eq!(levels.len(), 2);
}
