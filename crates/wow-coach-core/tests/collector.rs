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

// ---------------------------------------------------------------------------
// Roster gap
// ---------------------------------------------------------------------------

use wow_coach_core::gap::{self, KnownCharacter};

fn known(flavor: &str, realm: &str, name: &str) -> KnownCharacter {
    KnownCharacter {
        flavor_dir: Some(flavor.to_string()),
        realm: realm.to_string(),
        name: name.to_string(),
    }
}

#[test]
fn reports_characters_the_collector_has_never_seen() {
    let loaded = collector::load(&fixture("collector-v2.tbc.lua")).unwrap();
    let all = vec![
        known("_anniversary_", "Test Realm", "Testhero"),
        known("_anniversary_", "Test Realm", "Otherhero"),
        known("_anniversary_", "Test Realm", "Thirdhero"),
    ];

    let result = gap::find_gap(&all, &loaded.db.characters);
    assert_eq!(result.captured, vec!["Testhero"]);
    assert_eq!(result.total(), 3);
    assert!(!result.is_complete());

    let unseen: Vec<&str> = result
        .unseen
        .iter()
        .map(|character| character.name.as_str())
        .collect();
    assert_eq!(unseen, vec!["Otherhero", "Thirdhero"]);

    let summary = result.summary().expect("a gap should produce a summary");
    assert!(summary.contains("1 of 3"));
    assert!(summary.contains("Otherhero"));
}

#[test]
fn a_full_roster_reports_no_gap() {
    let loaded = collector::load(&fixture("collector-v2.tbc.lua")).unwrap();
    let all = vec![known("_anniversary_", "Test Realm", "Testhero")];
    let result = gap::find_gap(&all, &loaded.db.characters);
    assert!(result.is_complete());
    assert_eq!(
        result.summary(),
        None,
        "with nothing missing there is nothing to say"
    );
}

#[test]
fn matching_ignores_case() {
    // Folder names and the game's own capitalisation do not always agree.
    let loaded = collector::load(&fixture("collector-v2.tbc.lua")).unwrap();
    let all = vec![known("_anniversary_", "test realm", "TESTHERO")];
    assert!(gap::find_gap(&all, &loaded.db.characters).is_complete());
}

#[test]
fn the_same_name_on_another_realm_is_a_different_character() {
    let loaded = collector::load(&fixture("collector-v2.tbc.lua")).unwrap();
    let all = vec![
        known("_anniversary_", "Test Realm", "Testhero"),
        known("_anniversary_", "Another Realm", "Testhero"),
    ];
    let result = gap::find_gap(&all, &loaded.db.characters);
    assert_eq!(result.unseen.len(), 1, "the other realm's copy is unseen");
    assert_eq!(result.unseen[0].realm, "Another Realm");
}

#[test]
fn forevers_duplicate_folder_tree_does_not_invent_characters() {
    // The Forever client writes the real character folder under a numeric
    // realm as `Gravehexx-Gravehex`, and a stub under the ruleset name as
    // `Gravehexx`. Both turn up in a folder walk; only one is a character.
    let loaded = collector::load(&fixture("collector-v2.forever.lua")).unwrap();
    let all = vec![
        known("_classic_beta_", "Classic Beta PvE", "Trollmage"),
        known("_classic_beta_", "70", "Trollmage-Gemarolt"),
    ];
    let result = gap::find_gap(&all, &loaded.db.characters);
    assert!(
        result.is_complete(),
        "the hyphenated duplicate should not appear as a missing character, got {:?}",
        result.unseen
    );
}

#[test]
fn an_unrelated_hyphenated_name_is_kept() {
    // Dropping every hyphenated name would hide real characters; only ones
    // shadowed by a plainer name in the same flavor are duplicates.
    let loaded = collector::load(&fixture("collector-v2.tbc.lua")).unwrap();
    let all = vec![
        known("_anniversary_", "Test Realm", "Testhero"),
        known("_anniversary_", "Test Realm", "Jean-Luc"),
    ];
    let result = gap::find_gap(&all, &loaded.db.characters);
    assert_eq!(result.unseen.len(), 1);
    assert_eq!(result.unseen[0].name, "Jean-Luc");
}

#[test]
fn knowing_nothing_about_the_roster_reports_no_false_gap() {
    // A client that cannot enumerate folders passes an empty list. That must
    // not read as "every character is missing", nor as a complete roster it
    // has no evidence for.
    let loaded = collector::load(&fixture("collector-v2.tbc.lua")).unwrap();
    let result = gap::find_gap(&[], &loaded.db.characters);
    assert!(result.is_complete());
    assert_eq!(result.captured.len(), 1);
    assert_eq!(result.total(), 1);
}

// ---------------------------------------------------------------------------
// Shapes real files actually contain
// ---------------------------------------------------------------------------

#[test]
fn an_empty_list_field_may_arrive_as_an_empty_table() {
    // Lua's `{}` is both the empty list and the empty record, and nothing in
    // the text distinguishes them. A character whose bags are empty writes
    // `["contents"] = {}`, which is a perfectly good file. Found by the first
    // real capture, which the fixtures had not covered.
    let source = r#"
        WoWCoachCollectorDB = {
            ["schemaVersion"] = 2,
            ["characters"] = {
                ["k"] = {
                    ["name"] = "Emptybags",
                    ["inventory"] = { ["freeSlots"] = 70, ["contents"] = {}, ["bags"] = {} },
                    ["skills"] = {},
                    ["quests"] = {},
                },
            },
        }
    "#;
    let loaded = collector::load(source).expect("an empty bag is not a broken file");
    let record = loaded.db.characters.get("k").unwrap();
    let inventory = record.inventory.as_ref().unwrap();
    assert_eq!(inventory.contents.as_deref(), Some(&[][..]));
    assert_eq!(record.skills.as_deref(), Some(&[][..]));
    assert_eq!(record.quest_count(), Some(0));
}

#[test]
fn a_populated_table_where_a_list_belongs_is_still_an_error() {
    // Tolerating the empty case must not tolerate a genuine shape mismatch.
    let source = r#"
        WoWCoachCollectorDB = {
            ["characters"] = { ["k"] = { ["skills"] = { ["Alchemy"] = 225 } } },
        }
    "#;
    match collector::load(source) {
        Err(LoadError::Shape(message)) => {
            assert!(
                message.contains("expected a list"),
                "the error should say what was wrong: {message}"
            );
        }
        other => panic!("expected a shape error, got {other:?}"),
    }
}

#[test]
fn reads_arrays_written_the_way_wow_writes_them() {
    // WoW's serializer emits array entries as bare values, not `[1] = ...`.
    // The fixtures were generated with explicit indices and so never exercised
    // this until a real file did.
    let source = r#"
        WoWCoachCollectorDB = {
            ["characters"] = {
                ["k"] = {
                    ["name"] = "Barearrays",
                    ["skills"] = {
                        { ["name"] = "Alchemy", ["rank"] = 225, ["maxRank"] = 300 },
                        { ["name"] = "Herbalism", ["rank"] = 150, ["maxRank"] = 300 },
                    },
                },
            },
        }
    "#;
    let loaded = collector::load(source).expect("bare array entries are the normal case");
    let skills = loaded.db.characters["k"].skills.as_ref().unwrap();
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[1].name.as_deref(), Some("Herbalism"));
}
