//! Tests for the travel view.
//!
//! The claim under test: the collector reports which spells are known, and
//! every judgement about what those spells mean is made here. Two distinctions
//! carry the module — a spell not known versus a spellbook not captured, and a
//! destination this faction can use versus one it cannot.

use std::collections::BTreeMap;

use wow_coach_core::collector::{CharacterRecord, KnownSpell};
use wow_coach_core::roster::{self, Role, RosterConfig};
use wow_coach_core::travel::{self, Kind, Side};

fn character(name: &str, faction: &str, spells: Option<&[(u32, &str)]>) -> CharacterRecord {
    CharacterRecord {
        name: Some(name.to_string()),
        level: Some(40),
        realm: Some("Dreamscythe".to_string()),
        faction: Some(faction.to_string()),
        spells: spells.map(|list| {
            list.iter()
                .map(|(id, name)| KnownSpell {
                    spell_id: Some(*id),
                    name: Some((*name).to_string()),
                })
                .collect()
        }),
        ..Default::default()
    }
}

fn characters(records: Vec<CharacterRecord>) -> BTreeMap<String, CharacterRecord> {
    records
        .into_iter()
        .map(|record| (record.name.clone().unwrap_or_default(), record))
        .collect()
}

#[test]
fn a_known_teleport_names_its_destination() {
    let records = characters(vec![character(
        "Dardring",
        "Horde",
        Some(&[
            (3567, "Teleport: Orgrimmar"),
            (3566, "Teleport: Thunder Bluff"),
        ]),
    )]);
    let config = RosterConfig::default();
    let entries = roster::roster(&records, &config);
    let plan = travel::plan(&entries);

    assert_eq!(
        plan.by_destination.get("Orgrimmar"),
        Some(&vec!["Dardring".to_string()])
    );
    assert_eq!(
        plan.by_destination.get("Thunder Bluff"),
        Some(&vec!["Dardring".to_string()])
    );
    assert!(plan.characters[0].reaches("orgrimmar"), "case insensitive");
}

#[test]
fn never_looking_at_a_spellbook_is_not_the_same_as_finding_nothing() {
    // The distinction the whole module turns on. Both characters offer no
    // travel, and only one of them supports saying "this character cannot get
    // you there" — the other has simply never been logged in since the addon
    // learned to look.
    let records = characters(vec![
        character("Seen", "Horde", Some(&[])),
        character("Unseen", "Horde", None),
    ]);
    let config = RosterConfig::default();
    let entries = roster::roster(&records, &config);
    let plan = travel::plan(&entries);

    let seen = plan
        .characters
        .iter()
        .find(|c| c.name == "Seen")
        .expect("present");
    let unseen = plan
        .characters
        .iter()
        .find(|c| c.name == "Unseen")
        .expect("present");

    assert!(seen.spells.is_empty());
    assert!(unseen.spells.is_empty());
    assert!(seen.spellbook_seen, "this one was looked at");
    assert!(!unseen.spellbook_seen, "this one was not");

    assert!(
        plan.caveats
            .iter()
            .any(|caveat| caveat.contains("Unseen") && !caveat.contains("Seen,")),
        "only the uncaptured character is named: {:?}",
        plan.caveats
    );
}

#[test]
fn a_destination_the_other_faction_owns_is_not_offered() {
    // Sending an Alliance character to Orgrimmar is not a slow route, it is a
    // death. A capture holding the wrong side's spell is a bug upstream, and
    // the answer is to drop it rather than to pass it on.
    let records = characters(vec![character(
        "Fizzlemere",
        "Alliance",
        Some(&[(3561, "Teleport: Stormwind"), (3567, "Teleport: Orgrimmar")]),
    )]);
    let config = RosterConfig::default();
    let entries = roster::roster(&records, &config);
    let plan = travel::plan(&entries);

    assert!(plan.by_destination.contains_key("Stormwind"));
    assert!(
        !plan.by_destination.contains_key("Orgrimmar"),
        "got {:?}",
        plan.by_destination
    );
}

#[test]
fn shattrath_is_offered_to_both_sides() {
    for faction in ["Alliance", "Horde"] {
        let records = characters(vec![character(
            "Mage",
            faction,
            Some(&[(33690, "Teleport: Shattrath")]),
        )]);
        let config = RosterConfig::default();
        let entries = roster::roster(&records, &config);
        let plan = travel::plan(&entries);
        assert!(
            plan.by_destination.contains_key("Shattrath"),
            "{faction} should reach Shattrath"
        );
    }
}

#[test]
fn an_unknown_faction_is_not_a_reason_to_hide_a_route() {
    // A capture that did not record the faction is a gap in the data. Hiding a
    // real option because of it would be the wrong way to be careful.
    let mut record = character("Mystery", "Horde", Some(&[(3567, "Teleport: Orgrimmar")]));
    record.faction = None;
    let records = characters(vec![record]);
    let config = RosterConfig::default();
    let entries = roster::roster(&records, &config);
    let plan = travel::plan(&entries);

    assert!(plan.by_destination.contains_key("Orgrimmar"));
}

#[test]
fn a_portal_is_listed_ahead_of_a_teleport_to_the_same_place() {
    // A portal takes whoever is standing there, so it answers strictly more
    // than a teleport does. Ordering is the whole recommendation here.
    let records = characters(vec![
        character("Porter", "Horde", Some(&[(11417, "Portal: Orgrimmar")])),
        character("Solo", "Horde", Some(&[(3567, "Teleport: Orgrimmar")])),
    ]);
    let config = RosterConfig::default();
    let entries = roster::roster(&records, &config);
    let plan = travel::plan(&entries);

    let who = plan.by_destination.get("Orgrimmar").expect("reachable");
    assert_eq!(who[0], "Porter (portal)", "got {who:?}");
    assert_eq!(who[1], "Solo");
}

#[test]
fn a_bank_alt_still_counts_for_travel() {
    // The character crafting just told you to visit. Leaving the rotation was
    // never meant to hide it from the errand it exists for.
    let records = characters(vec![character(
        "Drektagand",
        "Horde",
        Some(&[(3567, "Teleport: Orgrimmar")]),
    )]);
    let mut config = RosterConfig::default();
    config.set_role("Drektagand", Role::Bank);
    let entries = roster::roster(&records, &config);
    let plan = travel::plan(&entries);

    assert_eq!(
        plan.by_destination.get("Orgrimmar"),
        Some(&vec!["Drektagand".to_string()])
    );
}

#[test]
fn a_travel_spell_this_build_does_not_know_is_reported_not_dropped() {
    // The table is a rule and rules go out of date. An id this version has
    // never heard of is a gap in the table, and saying so beats a silent
    // absence that reads as "you cannot go there".
    let records = characters(vec![character(
        "Future",
        "Horde",
        Some(&[(999_999, "Teleport: Dalaran")]),
    )]);
    let config = RosterConfig::default();
    let entries = roster::roster(&records, &config);
    let plan = travel::plan(&entries);

    assert!(plan.by_destination.is_empty(), "it is not guessed at");
    assert!(
        plan.caveats
            .iter()
            .any(|caveat| caveat.contains("Teleport: Dalaran") && caveat.contains("999999")),
        "got {:?}",
        plan.caveats
    );
}

#[test]
fn an_ordinary_spell_is_not_mistaken_for_travel() {
    // The name check exists only to flag gaps in the table. It must not fire
    // on the rest of a spellbook, which is most of what it will ever see.
    let records = characters(vec![character(
        "Mage",
        "Horde",
        Some(&[(133, "Fireball"), (1449, "Arcane Explosion")]),
    )]);
    let config = RosterConfig::default();
    let entries = roster::roster(&records, &config);
    let plan = travel::plan(&entries);

    assert!(plan.by_destination.is_empty());
    assert!(
        !plan.caveats.iter().any(|c| c.contains("travel table")),
        "got {:?}",
        plan.caveats
    );
}

#[test]
fn the_table_has_no_duplicate_ids_and_every_entry_resolves() {
    // A duplicated id would make one of the two entries unreachable, and which
    // one wins would depend on ordering nobody is watching.
    let mut seen = std::collections::BTreeSet::new();
    for entry in travel::all() {
        assert!(
            seen.insert(entry.spell_id),
            "spell {} appears twice",
            entry.spell_id
        );
        assert_eq!(
            travel::spell(entry.spell_id).map(|found| found.destination),
            Some(entry.destination)
        );
        assert!(!entry.destination.is_empty());
    }
    assert_eq!(seen.len(), travel::all().len());
}

#[test]
fn every_city_has_both_a_teleport_and_a_portal() {
    // A destination with only one of the two is the shape a missing id makes,
    // and it is invisible without checking: the table still answers, just less
    // well than the game does.
    let mut kinds: BTreeMap<&str, Vec<Kind>> = BTreeMap::new();
    for entry in travel::all() {
        kinds.entry(entry.destination).or_default().push(entry.kind);
    }
    for (destination, mut found) in kinds {
        found.sort();
        assert_eq!(
            found,
            vec![Kind::Teleport, Kind::Portal],
            "{destination} is missing one of the pair"
        );
    }
}

#[test]
fn each_side_has_the_same_number_of_cities() {
    // Classic and Burning Crusade give both factions four capitals apiece.
    // Lopsided counts mean a missing or misfiled id rather than lore.
    let count = |side: Side, kind: Kind| {
        travel::all()
            .iter()
            .filter(|entry| entry.side == side && entry.kind == kind)
            .count()
    };
    assert_eq!(count(Side::Alliance, Kind::Teleport), 4);
    assert_eq!(count(Side::Horde, Kind::Teleport), 4);
    assert_eq!(count(Side::Alliance, Kind::Portal), 4);
    assert_eq!(count(Side::Horde, Kind::Portal), 4);
    assert_eq!(count(Side::Neutral, Kind::Teleport), 1, "Shattrath");
    assert_eq!(count(Side::Neutral, Kind::Portal), 1);
}
