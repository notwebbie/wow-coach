//! Reading Auctionator's price database.
//!
//! The fixtures here are built the way the game writes them — CBOR, then Lua
//! string escaping, then raw bytes — rather than hand-written. A fixture that
//! is easy to type is usually a fixture in a format nothing produces, and that
//! mistake has already cost this project once.

use wow_coach_core::auctionator::{self, PriceError};

/// Escape bytes the way the game's SavedVariables writer does: five escapes,
/// everything else raw, NUL as a three-digit decimal.
fn lua_escape(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for byte in bytes {
        match byte {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'"' => out.extend_from_slice(b"\\\""),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            0 => out.extend_from_slice(b"\\000"),
            other => out.push(*other),
        }
    }
    out
}

fn cbor(value: &ciborium::Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).expect("encode");
    bytes
}

fn bytes_key(text: &str) -> ciborium::Value {
    // LibCBOR writes Lua strings as CBOR byte strings, not text strings. The
    // real database is full of these, so the fixtures must be too.
    ciborium::Value::Bytes(text.as_bytes().to_vec())
}

fn int(value: i64) -> ciborium::Value {
    ciborium::Value::Integer(value.into())
}

/// One item record in Auctionator's shape: m, h, l, a.
fn item(
    min: i64,
    high: &[(&str, i64)],
    low: &[(&str, i64)],
    available: &[(&str, i64)],
) -> ciborium::Value {
    let day_map = |entries: &[(&str, i64)]| {
        if entries.is_empty() {
            // An empty Lua table serializes as an empty array, not an empty
            // map. This is the same `{}` ambiguity the collector schema hits.
            ciborium::Value::Array(Vec::new())
        } else {
            ciborium::Value::Map(
                entries
                    .iter()
                    .map(|(day, value)| (bytes_key(day), int(*value)))
                    .collect(),
            )
        }
    };
    ciborium::Value::Map(vec![
        (bytes_key("m"), int(min)),
        (bytes_key("h"), day_map(high)),
        (bytes_key("l"), day_map(low)),
        (bytes_key("a"), day_map(available)),
    ])
}

fn realm_blob(entries: Vec<(&str, ciborium::Value)>) -> Vec<u8> {
    let mut map: Vec<(ciborium::Value, ciborium::Value)> = entries
        .into_iter()
        .map(|(key, value)| (bytes_key(key), value))
        .collect();
    // The real file carries a version sentinel among the items.
    map.push((bytes_key("version"), int(2)));
    cbor(&ciborium::Value::Map(map))
}

fn file(realms: Vec<(&str, Vec<u8>)>, vendor: &[(u32, u64)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"\nAUCTIONATOR_CONFIG = {\n[\"debug\"] = false,\n}\n");
    out.extend_from_slice(b"AUCTIONATOR_PRICE_DATABASE = {\n");
    for (name, blob) in realms {
        out.extend_from_slice(format!("[\"{name}\"] = \"").as_bytes());
        out.extend_from_slice(&lua_escape(&blob));
        out.extend_from_slice(b"\",\n");
    }
    out.extend_from_slice(b"[\"__dbversion\"] = 8,\n}\n");
    out.extend_from_slice(b"AUCTIONATOR_VENDOR_PRICE_CACHE = {\n");
    for (id, price) in vendor {
        out.extend_from_slice(format!("[\"{id}\"] = {price},\n").as_bytes());
    }
    out.extend_from_slice(b"}\n");
    out
}

#[test]
fn reads_prices_out_of_a_realm_blob() {
    let bytes = file(
        vec![(
            "Dreamscythe Horde",
            realm_blob(vec![
                (
                    "2447",
                    item(10, &[("2460", 10), ("2457", 8)], &[], &[("2460", 747)]),
                ),
                (
                    "10023",
                    item(89991, &[("2460", 89991)], &[], &[("2460", 4)]),
                ),
            ]),
        )],
        &[(2447, 1), (10023, 4500)],
    );

    let db = auctionator::load(&bytes).expect("load");
    assert!(
        db.notes.is_empty(),
        "clean file should raise nothing: {:?}",
        db.notes
    );

    let realm = db.realms.get("Dreamscythe Horde").expect("realm");
    assert_eq!(realm.items.len(), 2, "the version sentinel is not an item");

    let peacebloom = realm.for_item(2447).expect("item by bare id");
    assert_eq!(peacebloom.last_seen_min, Some(10));
    assert_eq!(peacebloom.high.get(&2460), Some(&10));
    assert_eq!(peacebloom.available.get(&2460), Some(&747));
    assert_eq!(realm.last_scan_day(), Some(2460));

    assert_eq!(db.vendor.get(&10023), Some(&4500));
}

#[test]
fn an_absent_low_is_not_a_price_of_zero() {
    // Auctionator only stores `l` when it differs from `h`, to save space. A
    // reader that filled the gap with zero would report every item as having
    // once sold for nothing.
    let bytes = file(
        vec![(
            "Realm Horde",
            realm_blob(vec![("2447", item(10, &[("2460", 10)], &[], &[]))]),
        )],
        &[],
    );
    let db = auctionator::load(&bytes).expect("load");
    let prices = db.realms["Realm Horde"].for_item(2447).expect("item");
    assert!(prices.low.is_empty(), "absent must stay absent");
    assert_eq!(prices.last_seen_min, Some(10));
}

#[test]
fn survives_the_control_bytes_cbor_puts_in_a_lua_string() {
    // The whole reason this module does not use the restricted-subset parser:
    // a realm blob is full of raw NULs, quotes, newlines and high bytes, and
    // is not valid UTF-8. Prices chosen to force each of those into the
    // encoding.
    let awkward = realm_blob(vec![
        // 0x0A00 and 0x0D22 put a newline and a quote inside an integer.
        ("1", item(0x0A00, &[("2460", 0x0D22)], &[], &[])),
        // A backslash byte, and a price needing the full 8-byte integer form.
        ("2", item(0x5C5C, &[("2460", 0x00FF_FFFF_FFFF)], &[], &[])),
        ("3", item(0, &[], &[], &[])),
    ]);
    let bytes = file(vec![("Realm Alliance", awkward)], &[]);

    assert!(
        std::str::from_utf8(&bytes).is_err(),
        "the fixture must be the awkward shape it is testing, or it tests nothing"
    );

    let db = auctionator::load(&bytes).expect("load");
    let realm = &db.realms["Realm Alliance"];
    assert_eq!(realm.for_item(1).unwrap().last_seen_min, Some(0x0A00));
    assert_eq!(realm.for_item(1).unwrap().high[&2460], 0x0D22);
    assert_eq!(realm.for_item(2).unwrap().last_seen_min, Some(0x5C5C));
    assert_eq!(realm.for_item(2).unwrap().high[&2460], 0x00FF_FFFF_FFFF);
    assert_eq!(realm.for_item(3).unwrap().last_seen_min, Some(0));
}

#[test]
fn one_broken_realm_does_not_cost_the_others() {
    let mut bytes = file(
        vec![(
            "Good Horde",
            realm_blob(vec![("2447", item(10, &[("2460", 10)], &[], &[]))]),
        )],
        &[],
    );
    // Splice in a realm whose blob is not CBOR at all.
    let broken = b"[\"Broken Alliance\"] = \"not cbor at all\",\n";
    let at = bytes.windows(2).position(|w| w == b"[\"").unwrap();
    let at = at
        + bytes[at..]
            .windows(2)
            .position(|w| w == b"\n[")
            .unwrap_or(0);
    bytes.splice(at + 1..at + 1, broken.iter().copied());

    let db = auctionator::load(&bytes).expect("load");
    assert!(
        db.realms.contains_key("Good Horde"),
        "the readable realm survives"
    );
    assert!(
        db.notes.iter().any(|note| note.contains("Broken Alliance")),
        "and the unreadable one is reported, not swallowed: {:?}",
        db.notes
    );
}

#[test]
fn a_file_that_is_not_auctionators_says_so() {
    let error = auctionator::load(b"SOME_OTHER_ADDON = {\n}\n").unwrap_err();
    assert_eq!(
        error,
        PriceError::MissingGlobal("AUCTIONATOR_PRICE_DATABASE")
    );
    assert!(error.to_string().contains("Auctionator"));
}

#[test]
fn realm_key_needs_both_halves() {
    // Horde and Alliance have separate auction houses and separate prices.
    // Guessing the faction would value a roster at the wrong market.
    assert_eq!(
        auctionator::realm_key(Some("Dreamscythe"), Some("Horde")).as_deref(),
        Some("Dreamscythe Horde")
    );
    assert_eq!(auctionator::realm_key(Some("Dreamscythe"), None), None);
    assert_eq!(auctionator::realm_key(None, Some("Horde")), None);
}

#[test]
fn an_escaped_quote_does_not_end_the_blob() {
    // `\"` inside the literal must not terminate it, while `\\"` must. Getting
    // this wrong truncates a realm silently rather than failing.
    let blob = realm_blob(vec![
        ("1", item(0x225C, &[("2460", 0x5C22)], &[], &[])),
        ("2", item(0x2222, &[("2460", 0x5C5C)], &[], &[])),
    ]);
    let bytes = file(vec![("Quoted Horde", blob)], &[]);
    let db = auctionator::load(&bytes).expect("load");
    let realm = &db.realms["Quoted Horde"];
    assert_eq!(
        realm.items.len(),
        2,
        "nothing was lost to a premature quote"
    );
    assert_eq!(realm.for_item(2).unwrap().high[&2460], 0x5C5C);
}
