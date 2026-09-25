//! Tests for the roster-wide economy view.
//!
//! The claim this file defends: a bank alt is invisible to the play rotation
//! and fully visible to the economy. That is the whole reason the role exists,
//! and it is the exact behaviour the earlier prototype got backwards.

use std::collections::BTreeMap;

use wow_coach_core::auctionator::{ItemPrices, PriceDb, RealmPrices};
use wow_coach_core::collector::{CharacterRecord, Inventory, ItemStack, ProfessionRecipes, Recipe};
use wow_coach_core::economy;
use wow_coach_core::roster::{self, Role, RosterConfig};

fn faction_character(
    name: &str,
    faction: &str,
    gold_copper: u64,
    stacks: &[(u32, u32)],
) -> CharacterRecord {
    let mut record = character(name, gold_copper, stacks);
    record.faction = Some(faction.to_string());
    record
}

fn character(name: &str, gold_copper: u64, stacks: &[(u32, u32)]) -> CharacterRecord {
    // Stacks are given without names, which is the honest default: the client
    // only names an item it has already loaded.
    CharacterRecord {
        name: Some(name.to_string()),
        level: Some(60),
        realm: Some("Dreamscythe".to_string()),
        faction: Some("Horde".to_string()),
        money_copper: Some(gold_copper),
        inventory: Some(Inventory {
            bags: Some(Vec::new()),
            total_slots: Some(80),
            free_slots: Some(40),
            contents: Some(
                stacks
                    .iter()
                    .map(|(item_id, count)| ItemStack {
                        item_id: Some(*item_id),
                        count: Some(*count),
                        name: None,
                    })
                    .collect(),
            ),
        }),
        ..Default::default()
    }
}

fn with_profession(
    mut record: CharacterRecord,
    profession: &str,
    rank: u16,
    max_rank: u16,
    recipes: usize,
) -> CharacterRecord {
    let cache = ProfessionRecipes {
        captured_at: Some(1_800_000_000),
        rank: Some(rank),
        max_rank: Some(max_rank),
        recipes: Some((0..recipes).map(|_| Recipe::default()).collect()),
    };
    record
        .recipes
        .get_or_insert_with(BTreeMap::new)
        .insert(profession.to_string(), cache);
    record
}

fn characters(records: Vec<CharacterRecord>) -> BTreeMap<String, CharacterRecord> {
    records
        .into_iter()
        .map(|record| (record.name.clone().unwrap(), record))
        .collect()
}

/// A price database where one item's latest scan disagrees wildly with its
/// history — the Copper Rod case from live data.
fn prices_with_history(item_id: u32, last_seen: u64, history: &[(i64, u64)]) -> PriceDb {
    let items = [(
        item_id.to_string(),
        ItemPrices {
            last_seen_min: Some(last_seen),
            high: history.iter().copied().collect(),
            low: BTreeMap::new(),
            available: BTreeMap::new(),
        },
    )]
    .into_iter()
    .collect();
    PriceDb {
        realms: [("Dreamscythe Horde".to_string(), RealmPrices { items })]
            .into_iter()
            .collect(),
        vendor: BTreeMap::new(),
        notes: Vec::new(),
    }
}

/// A price database holding one realm, with prices last seen on `scan_day`.
fn prices(entries: &[(u32, u64, i64)], scan_day: i64) -> PriceDb {
    let mut items = BTreeMap::new();
    for (item_id, price, seen_day) in entries {
        items.insert(
            item_id.to_string(),
            ItemPrices {
                last_seen_min: Some(*price),
                high: [(*seen_day, *price)].into_iter().collect(),
                low: BTreeMap::new(),
                available: BTreeMap::new(),
            },
        );
    }
    // Anchor the realm's latest scan even when no held item was in it.
    items.insert(
        "1".to_string(),
        ItemPrices {
            last_seen_min: Some(1),
            high: [(scan_day, 1)].into_iter().collect(),
            low: BTreeMap::new(),
            available: BTreeMap::new(),
        },
    );
    PriceDb {
        realms: [("Dreamscythe Horde".to_string(), RealmPrices { items })]
            .into_iter()
            .collect(),
        vendor: BTreeMap::new(),
        notes: Vec::new(),
    }
}

#[test]
fn a_bank_alt_leaves_the_rotation_and_stays_in_the_economy() {
    // The headline claim. A bank alt holds the materials; hiding it would hide
    // the inventory it is being kept for.
    let roster_records = characters(vec![
        with_profession(
            character("Main", 50_000, &[(2447, 10)]),
            "Alchemy",
            300,
            300,
            40,
        ),
        with_profession(
            character("Vault", 900_000, &[(2447, 200), (2449, 60)]),
            "Mining",
            150,
            150,
            0,
        ),
    ]);
    let mut config = RosterConfig::default();
    config.set_role("Vault", Role::Bank);
    let entries = roster::roster(&roster_records, &config);

    // The play rotation sees one character.
    let advice = wow_coach_core::coaching::advise(&entries, true);
    assert_eq!(
        advice.play.len() + advice.park.len(),
        1,
        "only Main is in the rotation"
    );

    // The economy sees both, and both their professions.
    let survey = economy::survey(
        &entries,
        &prices(&[(2447, 10, 2460), (2449, 500, 2460)], 2460),
    );
    assert_eq!(survey.professions.len(), 2);
    assert!(survey
        .who_has("Mining")
        .iter()
        .any(|holder| holder.name == "Vault"));
    assert_eq!(survey.gold_copper, 950_000, "the bank alt's gold counts");

    let peacebloom = survey
        .holdings
        .iter()
        .find(|holding| holding.item_id == 2447)
        .expect("the shared stack is totalled");
    assert_eq!(peacebloom.count, 210, "10 on Main plus 200 in the vault");
    assert_eq!(
        peacebloom.held_by["Vault"], 200,
        "and it says where to go for it"
    );
}

#[test]
fn an_unpriced_item_is_not_a_worthless_one() {
    // "Auctionator has never seen this" and "this is worth nothing" are
    // different claims. Only one of them is supported by the data.
    let records = characters(vec![character("Main", 0, &[(2447, 10), (99999, 5)])]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(&entries, &prices(&[(2447, 100, 2460)], 2460));

    assert_eq!(survey.holdings.len(), 1);
    assert_eq!(
        survey.holdings_value, 1_000,
        "only the priced stack contributes"
    );
    assert_eq!(survey.unpriced.len(), 1);
    assert_eq!(survey.unpriced[0].item_id, 99999);
    assert!(survey.unpriced[0].value.is_none());
    assert!(
        survey
            .caveats
            .iter()
            .any(|caveat| caveat.contains("unpriced")),
        "and the reader is told: {:?}",
        survey.caveats
    );
}

#[test]
fn a_stale_price_is_flagged_rather_than_quietly_used() {
    let records = characters(vec![character("Main", 0, &[(2447, 1)])]);
    let entries = roster::roster(&records, &RosterConfig::default());

    // Last seen 30 days before the most recent scan.
    let survey = economy::survey(&entries, &prices(&[(2447, 100, 2430)], 2460));
    assert_eq!(survey.holdings[0].days_stale, Some(30));
    assert!(
        survey
            .caveats
            .iter()
            .any(|caveat| caveat.contains("week or more")),
        "{:?}",
        survey.caveats
    );

    // Seen in the latest scan: no complaint.
    let fresh = economy::survey(&entries, &prices(&[(2447, 100, 2460)], 2460));
    assert_eq!(fresh.holdings[0].days_stale, Some(0));
    assert!(!fresh
        .caveats
        .iter()
        .any(|caveat| caveat.contains("week or more")));
}

#[test]
fn a_realm_with_no_scan_is_named_rather_than_valued_at_nothing() {
    let records = characters(vec![character("Main", 0, &[(2447, 10)])]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(&entries, &PriceDb::default());

    assert!(survey.holdings.is_empty());
    assert_eq!(
        survey.unpriced.len(),
        1,
        "the stack is still reported as held"
    );
    assert!(
        survey
            .caveats
            .iter()
            .any(|caveat| caveat.contains("Dreamscythe Horde") && caveat.contains("scan")),
        "{:?}",
        survey.caveats
    );
}

#[test]
fn only_trade_skills_count_as_professions() {
    // The client's skill list also holds talent tabs, weapon skills, armour
    // proficiencies and languages. Real data reported "Affliction", "Cloth"
    // and "Language: Orcish" as professions needing training. The recipe cache
    // is the locale-independent evidence that a skill is a trade skill.
    let mut record = character("Main", 0, &[]);
    record.skills = Some(vec![wow_coach_core::collector::Skill {
        name: Some("Language: Orcish".to_string()),
        rank: Some(300),
        max_rank: Some(300),
        skill_id: None,
    }]);
    let record = with_profession(record, "Tailoring", 225, 225, 12);

    let records = characters(vec![record]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(&entries, &PriceDb::default());

    assert_eq!(survey.professions.len(), 1);
    assert_eq!(survey.professions[0].profession, "Tailoring");
    assert!(
        survey.professions[0].needs_training(),
        "225/225 needs a trainer"
    );
    assert_eq!(survey.professions[0].recipes_known, Some(12));
}

#[test]
fn a_character_whose_bags_were_not_captured_is_named() {
    // Counting an uncaptured character as holding nothing would make the total
    // look complete when it is not.
    let mut blind = character("Unseen", 0, &[]);
    blind.inventory = None;
    let records = characters(vec![character("Main", 0, &[(2447, 1)]), blind]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(&entries, &prices(&[(2447, 100, 2460)], 2460));

    assert!(
        survey
            .caveats
            .iter()
            .any(|caveat| caveat.contains("Unseen")),
        "{:?}",
        survey.caveats
    );
}

#[test]
fn coin_splits_the_way_the_game_shows_it() {
    assert_eq!(economy::coin(0), (0, 0, 0));
    assert_eq!(economy::coin(9_999), (0, 99, 99));
    assert_eq!(economy::coin(10_000), (1, 0, 0));
    assert_eq!(economy::coin(1_234_567), (123, 45, 67));
}

#[test]
fn a_parked_character_still_counts_in_the_economy() {
    // Parked means "not now", not "not mine". Its gold and materials are still
    // the player's, and it is still the one holding them.
    let records = characters(vec![character("Resting", 777, &[(2447, 3)])]);
    let mut config = RosterConfig::default();
    config.set_role("Resting", Role::Parked);
    let entries = roster::roster(&records, &config);
    let survey = economy::survey(&entries, &prices(&[(2447, 100, 2460)], 2460));

    assert_eq!(survey.gold_copper, 777);
    assert_eq!(survey.holdings_value, 300);
}

#[test]
fn an_item_name_is_used_when_the_capture_had_one_and_never_as_a_key() {
    // Names are locale-dependent and often missing, so they are display only.
    // Two characters holding the same item id must still total as one holding
    // even when only one of them managed to name it.
    let mut named = character("Named", 0, &[]);
    named.inventory = Some(Inventory {
        bags: Some(Vec::new()),
        total_slots: Some(16),
        free_slots: Some(15),
        contents: Some(vec![ItemStack {
            item_id: Some(2447),
            count: Some(5),
            name: Some("Peacebloom".to_string()),
        }]),
    });
    let records = characters(vec![named, character("Nameless", 0, &[(2447, 7)])]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(&entries, &prices(&[(2447, 100, 2460)], 2460));

    assert_eq!(survey.holdings.len(), 1, "one item id is one holding");
    assert_eq!(survey.holdings[0].count, 12);
    assert_eq!(survey.holdings[0].name.as_deref(), Some("Peacebloom"));
}

#[test]
fn an_item_with_no_name_anywhere_is_reported_rather_than_hidden() {
    let records = characters(vec![character("Main", 0, &[(2447, 1)])]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(&entries, &prices(&[(2447, 100, 2460)], 2460));

    assert_eq!(survey.holdings[0].item_id, 2447, "the id is still the fact");
    assert!(survey.holdings[0].name.is_none());
    assert!(
        survey
            .caveats
            .iter()
            .any(|caveat| caveat.contains("no name")),
        "{:?}",
        survey.caveats
    );
}

#[test]
fn two_factions_are_two_markets_and_are_never_pooled() {
    // Horde and Alliance run separate auction houses on the same realm. In
    // real data the same item was 27 copper on one and 57 on the other. One
    // pooled row would price half the roster at the wrong market, silently and
    // in the direction nobody thinks to check.
    let records = characters(vec![
        faction_character("Horde", "Horde", 0, &[(2453, 10)]),
        faction_character("Ally", "Alliance", 0, &[(2453, 10)]),
    ]);
    let entries = roster::roster(&records, &RosterConfig::default());

    let mut prices = prices(&[(2453, 27, 2460)], 2460);
    let alliance = RealmPrices {
        items: [(
            "2453".to_string(),
            ItemPrices {
                last_seen_min: Some(57),
                high: [(2460, 57)].into_iter().collect(),
                low: BTreeMap::new(),
                available: BTreeMap::new(),
            },
        )]
        .into_iter()
        .collect(),
    };
    prices
        .realms
        .insert("Dreamscythe Alliance".to_string(), alliance);

    let survey = economy::survey(&entries, &prices);

    assert_eq!(
        survey.holdings.len(),
        2,
        "one item id, two markets, two rows"
    );
    let horde = survey
        .holdings
        .iter()
        .find(|holding| holding.market.as_deref() == Some("Dreamscythe Horde"))
        .expect("the Horde row");
    let ally = survey
        .holdings
        .iter()
        .find(|holding| holding.market.as_deref() == Some("Dreamscythe Alliance"))
        .expect("the Alliance row");

    assert_eq!(horde.count, 10);
    assert_eq!(horde.unit_price, Some(27));
    assert_eq!(ally.unit_price, Some(57), "priced at its own auction house");
    assert_eq!(survey.holdings_value, 270 + 570);

    assert!(
        survey
            .caveats
            .iter()
            .any(|caveat| caveat.contains("separate auction houses")),
        "and the reader is told the roster spans two: {:?}",
        survey.caveats
    );
}

#[test]
fn one_faction_still_totals_into_a_single_row() {
    // The market split must not fragment the ordinary case.
    let records = characters(vec![
        faction_character("A", "Horde", 0, &[(2447, 3)]),
        faction_character("B", "Horde", 0, &[(2447, 4)]),
    ]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(&entries, &prices(&[(2447, 100, 2460)], 2460));

    assert_eq!(survey.holdings.len(), 1);
    assert_eq!(survey.holdings[0].count, 7);
    assert!(
        !survey
            .caveats
            .iter()
            .any(|caveat| caveat.contains("separate auction houses")),
        "and no warning about markets it does not span"
    );
}

#[test]
fn one_joke_listing_does_not_get_to_be_the_market() {
    // Real data: a Copper Rod read 1,399 gold from the latest scan against
    // about 20 silver on each of the three days before it. Somebody listed one
    // absurdly and it became "the price". The figure is reported, because it
    // is what was seen, but it is flagged and a second total is offered.
    let records = characters(vec![character("Main", 0, &[(6217, 1)])]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(
        &entries,
        &prices_with_history(
            6217,
            13_993_880,
            &[(2442, 1_694), (2457, 2_969), (2460, 13_993_880)],
        ),
    );

    let holding = &survey.holdings[0];
    assert_eq!(
        holding.unit_price,
        Some(13_993_880),
        "what was seen is reported"
    );
    assert_eq!(
        holding.typical_price,
        Some(2_969),
        "the median of the days on record"
    );
    assert!(holding.price_looks_like_an_outlier());
    assert_eq!(survey.holdings_value, 13_993_880);
    assert_eq!(survey.holdings_value_typical, 2_969);
    assert!(
        holding.unit_price.unwrap() as f64 / holding.typical_price.unwrap() as f64 > 1_000.0,
        "this is the egregious case, not a borderline one"
    );
    assert!(
        survey
            .caveats
            .iter()
            .any(|caveat| caveat.contains("odd listing")),
        "{:?}",
        survey.caveats
    );
}

#[test]
fn an_ordinary_price_is_not_called_an_outlier() {
    // Prices move. A tool that flags normal volatility trains the reader to
    // ignore the flag, which costs more than not having one.
    let records = characters(vec![character("Main", 0, &[(2447, 1)])]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(
        &entries,
        &prices_with_history(2447, 150, &[(2458, 90), (2459, 100), (2460, 150)]),
    );

    assert!(!survey.holdings[0].price_looks_like_an_outlier());
    assert!(!survey
        .caveats
        .iter()
        .any(|caveat| caveat.contains("odd listing")));
}

#[test]
fn an_item_with_no_history_is_not_judged_against_one() {
    // No history is not agreement and not disagreement. Both totals fall back
    // to the same figure so they stay comparable.
    let records = characters(vec![character("Main", 0, &[(2447, 2)])]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let mut db = prices_with_history(2447, 500, &[]);
    db.realms
        .get_mut("Dreamscythe Horde")
        .unwrap()
        .items
        .insert(
            "1".to_string(),
            ItemPrices {
                last_seen_min: Some(1),
                high: [(2460, 1)].into_iter().collect(),
                low: BTreeMap::new(),
                available: BTreeMap::new(),
            },
        );
    let survey = economy::survey(&entries, &db);

    let holding = survey
        .holdings
        .iter()
        .find(|holding| holding.item_id == 2447)
        .expect("held");
    assert!(holding.typical_price.is_none());
    assert!(!holding.price_looks_like_an_outlier());
    assert_eq!(survey.holdings_value, survey.holdings_value_typical);
}

#[test]
fn a_partial_scan_is_not_mistaken_for_a_bad_listing() {
    // Auctionator's daily figure reflects how complete that day's scan was,
    // not only the market. Real data: an item read 29 copper on a day the
    // player ran a targeted search and 1,999 on a day they ran a full one — a
    // 29x spread with nothing wrong in it. Flagging that would put a marker on
    // a third of the rows and train the reader to ignore all of them.
    let records = characters(vec![character("Main", 0, &[(9451, 11)])]);
    let entries = roster::roster(&records, &RosterConfig::default());
    let survey = economy::survey(
        &entries,
        &prices_with_history(
            9451,
            1_999,
            &[(2440, 29), (2442, 83), (2457, 56), (2460, 1_999)],
        ),
    );

    assert!(
        !survey.holdings[0].price_looks_like_an_outlier(),
        "a scan-completeness artifact is not a mis-listing"
    );
    assert_eq!(
        survey.holdings_value, survey.holdings_value_typical,
        "and nothing is substituted for it"
    );
}
