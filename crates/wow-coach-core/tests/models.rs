use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use wow_coach_core::{
    AccountIdentity, CharacterIdentity, CharacterSnapshot, GameFlavor, HistoryEntry, LegacyReportV3,
};

#[test]
fn character_keys_do_not_collide_when_components_contain_separators() {
    let left = CharacterIdentity::new(
        AccountIdentity::new("install:a", "account"),
        GameFlavor::Anniversary,
        "US",
        "Realm:One",
        "Hero",
    );
    let right = CharacterIdentity::new(
        AccountIdentity::new("install", "a:account"),
        GameFlavor::Anniversary,
        "US",
        "Realm",
        "One:Hero",
    );

    assert_ne!(left.storage_key(), right.storage_key());
}

#[test]
fn identity_normalizes_case_and_surrounding_whitespace() {
    let first = CharacterIdentity::new(
        AccountIdentity::new(" Local Install ", " Example Account "),
        GameFlavor::TbcClassic,
        " us ",
        " Example Realm ",
        " Examplemage ",
    );
    let second = CharacterIdentity::new(
        AccountIdentity::new("local install", "example account"),
        GameFlavor::TbcClassic,
        "US",
        "example realm",
        "examplemage",
    );

    assert_eq!(first.storage_key(), second.storage_key());
}

#[test]
fn custom_flavor_keys_are_namespaced_from_every_builtin_flavor() {
    let builtins = [
        (GameFlavor::Anniversary, "anniversary"),
        (GameFlavor::TbcClassic, "tbc_classic"),
        (GameFlavor::ClassicEra, "classic_era"),
        (GameFlavor::Retail, "retail"),
    ];

    for (builtin, reserved_name) in builtins {
        let builtin_identity = CharacterIdentity::new(
            AccountIdentity::new("install", "account"),
            builtin,
            "us",
            "realm",
            "hero",
        );
        let custom_identity = CharacterIdentity::new(
            AccountIdentity::new("install", "account"),
            GameFlavor::Other(reserved_name.into()),
            "us",
            "realm",
            "hero",
        );

        assert_ne!(
            builtin_identity.storage_key(),
            custom_identity.storage_key()
        );
    }
}

#[test]
fn custom_flavor_keys_normalize_case_and_surrounding_whitespace() {
    let first = CharacterIdentity::new(
        AccountIdentity::new("install", "account"),
        GameFlavor::Other("  Season Of Discovery  ".into()),
        "us",
        "realm",
        "hero",
    );
    let second = CharacterIdentity::new(
        AccountIdentity::new("install", "account"),
        GameFlavor::Other("season of discovery".into()),
        "us",
        "realm",
        "hero",
    );

    assert_eq!(first.storage_key(), second.storage_key());
}

#[test]
fn deserialized_identity_has_the_same_key_as_constructor_normalized_identity() {
    let decoded: CharacterIdentity = serde_json::from_str(
        r#"{
            "account": {
                "installation_id": " Local Install ",
                "account_id": " Example Account "
            },
            "game_flavor": { "other": " Season Of Discovery " },
            "region": " US ",
            "realm": " Example Realm ",
            "character_name": " ExampleMage "
        }"#,
    )
    .unwrap();
    let canonical = CharacterIdentity::new(
        AccountIdentity::new("local install", "example account"),
        GameFlavor::Other("season of discovery".into()),
        "us",
        "example realm",
        "examplemage",
    );

    assert_eq!(decoded.storage_key(), canonical.storage_key());
}

#[test]
fn snapshot_round_trips_as_versioned_json() {
    let identity = CharacterIdentity::new(
        AccountIdentity::new("local-install", "example-account"),
        GameFlavor::Anniversary,
        "US",
        "Example Realm",
        "Examplemage",
    );
    let snapshot = CharacterSnapshot {
        schema_version: 1,
        identity,
        captured_at: Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap(),
        level: 18,
        class_id: Some(8),
        race_id: Some(1),
        faction: Some("Alliance".into()),
        zone: Some("Westfall".into()),
        money_copper: Some(104_800),
        xp: Some(8_754),
        max_xp: Some(17_800),
        rested_xp: Some(258),
        professions: BTreeMap::new(),
    };
    let encoded = serde_json::to_string(&snapshot).unwrap();
    let decoded: CharacterSnapshot = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, snapshot);
}

#[test]
fn history_entry_rejects_mismatched_identity() {
    let first = CharacterIdentity::new(
        AccountIdentity::new("local", "one"),
        GameFlavor::Anniversary,
        "US",
        "Example Realm",
        "Examplemage",
    );
    let second = CharacterIdentity::new(
        AccountIdentity::new("local", "two"),
        GameFlavor::Anniversary,
        "US",
        "Example Realm",
        "Examplemage",
    );
    let snapshot = CharacterSnapshot::minimal(second, Utc::now(), 18);

    assert!(HistoryEntry::new(first, snapshot).is_err());
}

#[test]
fn imports_anonymized_legacy_schema_v3_shape() {
    let fixture = include_str!("../../../fixtures/legacy-report-v3.example.json");
    let report: LegacyReportV3 = serde_json::from_str(fixture).unwrap();

    assert_eq!(report.schema_version, 3);
    assert_eq!(report.accounts.len(), 1);
    assert_eq!(report.accounts[0].characters[0].name, "Examplemage");
}
