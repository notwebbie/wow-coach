//! Reading Auctionator's price database.
//!
//! Auctionator is the one outside addon this tool depends on, and the reason
//! is worth restating: our own collector only ever sees the scans the player
//! personally ran while it was installed. Auctionator holds every scan the
//! account has ever done, going back weeks. Re-implementing that would mean
//! asking the player to throw away history they already have.
//!
//! # Why this file does not use `lua.rs`
//!
//! `AUCTIONATOR_PRICE_DATABASE` does not hold tables. It holds one **CBOR
//! blob per realm-and-faction**, written into a Lua string literal — so the
//! file is full of raw control bytes and is not valid UTF-8 anywhere near
//! them. The restricted-subset parser works on `&str` and would refuse the
//! file before reaching a single price.
//!
//! So this module works at the byte level, and deliberately does not try to
//! be a Lua parser. It finds two named globals, reads string literals and
//! plain integer entries out of them, and stops. Anything else in the file is
//! none of its business. Narrowness is the safety property here: a format we
//! do not control is being read by code that can only do one thing with it.
//!
//! The blob itself is decoded with a real CBOR implementation rather than a
//! hand-rolled one. This is Auctionator's format, not ours; it can change
//! under us, and a subtly wrong reader that returns plausible numbers is far
//! worse than one that fails.
//!
//! # What the numbers mean
//!
//! Auctionator's own source documents the per-item record:
//!
//! - `m` — the last seen minimum buyout, per unit, in copper.
//! - `h` — day number to the *highest* low price seen that day.
//! - `l` — day number to the *lowest* low price seen that day, stored only
//!   when it differs from `h`, to save space. An absent `l` is not zero.
//! - `a` — day number to the highest quantity seen that day.
//!
//! A day number is `floor((now - 2020-01-01) / 86400)` evaluated in the
//! *player's local timezone*, because that is what the game's `time()` does.
//! This module therefore never converts a day number to a date. It compares
//! day numbers with each other, where the offset cancels out, and leaves
//! calendar arithmetic to nobody.

use std::collections::BTreeMap;

const PRICE_GLOBAL: &[u8] = b"AUCTIONATOR_PRICE_DATABASE = {";
const VENDOR_GLOBAL: &[u8] = b"AUCTIONATOR_VENDOR_PRICE_CACHE = {";

/// The whole price database, keyed by Auctionator's own realm key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PriceDb {
    /// Key is `"<Realm> <Faction>"`, e.g. `"Dreamscythe Horde"`.
    pub realms: BTreeMap<String, RealmPrices>,
    /// Item id to the price a vendor pays, in copper. Account-wide, and
    /// unrelated to the auction house.
    pub vendor: BTreeMap<u32, u64>,
    /// Non-fatal notes, surfaced rather than logged.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RealmPrices {
    /// Key is Auctionator's db key: a bare item id, `p:<species>` for a battle
    /// pet, `gr:<item>:<suffix>` for legacy random-suffix gear, or
    /// `g:<item>:<ilvl>` for modern gear above the item-level threshold.
    pub items: BTreeMap<String, ItemPrices>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemPrices {
    /// Last seen minimum buyout per unit, in copper.
    pub last_seen_min: Option<u64>,
    /// Day number to highest low price.
    pub high: BTreeMap<i64, u64>,
    /// Day number to lowest low price, where it differed from the high.
    pub low: BTreeMap<i64, u64>,
    /// Day number to highest quantity seen.
    pub available: BTreeMap<i64, u64>,
}

impl ItemPrices {
    /// The middle of the daily prices on record, which is what the item has
    /// actually been going for.
    ///
    /// `last_seen_min` is one observation and nothing more. When an item is
    /// thin on the auction house, a single joke listing becomes the whole
    /// market: a Copper Rod in real data reads 1,399 gold from the latest scan
    /// against 20 silver on each of the three days before it. A median over
    /// the retained days cannot be moved by one listing, which is exactly the
    /// property wanted here.
    ///
    /// `None` when there is no price history at all — never a guess.
    pub fn typical(&self) -> Option<u64> {
        if self.high.is_empty() {
            return None;
        }
        let mut days: Vec<u64> = self.high.values().copied().collect();
        days.sort_unstable();
        Some(days[days.len() / 2])
    }

    /// How far the last seen price is from the typical one, as a ratio.
    ///
    /// Returns `None` when there is nothing to compare against, which is not
    /// the same as agreement.
    pub fn divergence(&self) -> Option<f64> {
        let typical = self.typical()?;
        let last = self.last_seen_min?;
        if typical == 0 || last == 0 {
            return None;
        }
        let ratio = last as f64 / typical as f64;
        Some(if ratio >= 1.0 { ratio } else { 1.0 / ratio })
    }

    /// The most recent day this item was seen in a scan.
    pub fn last_seen_day(&self) -> Option<i64> {
        self.high.keys().chain(self.available.keys()).copied().max()
    }
}

impl RealmPrices {
    /// Look up a plain item id.
    ///
    /// Gear with a random suffix is also stored under a `gr:` key carrying the
    /// suffix, but the collector captures bag contents as bare item ids and
    /// cannot tell which suffix a stack has. Auctionator records the bare key
    /// too, and its own lookup falls back to exactly this, so a bare-id lookup
    /// is the honest one: right for materials and consumables, and a
    /// suffix-blind approximation for gear.
    pub fn for_item(&self, item_id: u32) -> Option<&ItemPrices> {
        self.items.get(&item_id.to_string())
    }

    /// The newest day number anywhere in this realm's data — that is, when
    /// the account last ran a scan.
    pub fn last_scan_day(&self) -> Option<i64> {
        self.items
            .values()
            .filter_map(ItemPrices::last_seen_day)
            .max()
    }
}

/// What went wrong reading the file.
#[derive(Debug, Clone, PartialEq)]
pub enum PriceError {
    /// The named global is not in the file at all.
    MissingGlobal(&'static str),
    /// A string literal began and never ended.
    UnterminatedString(String),
    /// The blob is not CBOR, or not CBOR we understand.
    Cbor { realm: String, detail: String },
}

impl std::fmt::Display for PriceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingGlobal(name) => write!(
                f,
                "{name} is not in this file — is it Auctionator's SavedVariables?"
            ),
            Self::UnterminatedString(realm) => {
                write!(f, "the entry for {realm} is missing its closing quote")
            }
            Self::Cbor { realm, detail } => {
                write!(f, "could not decode the price blob for {realm}: {detail}")
            }
        }
    }
}

impl std::error::Error for PriceError {}

/// Read a whole `Auctionator.lua`.
///
/// Takes bytes rather than a string on purpose: the file is not valid UTF-8,
/// and lossy conversion would corrupt the very blobs being read.
pub fn load(bytes: &[u8]) -> Result<PriceDb, PriceError> {
    let mut db = PriceDb::default();

    let price_body = global_body(bytes, PRICE_GLOBAL)
        .ok_or(PriceError::MissingGlobal("AUCTIONATOR_PRICE_DATABASE"))?;

    for (key, literal) in string_entries(price_body)? {
        // `__dbversion` sits beside the realms as a plain number, so it never
        // appears here; anything that is a string is a realm blob.
        let blob = unescape(&literal);
        match decode_realm(&blob) {
            Ok(prices) => {
                db.realms.insert(key, prices);
            }
            Err(detail) => {
                // One unreadable realm should not cost the player the rest.
                db.notes.push(format!(
                    "could not decode the price data for {key}: {detail}"
                ));
            }
        }
    }

    if let Some(vendor_body) = global_body(bytes, VENDOR_GLOBAL) {
        db.vendor = integer_entries(vendor_body);
    }

    Ok(db)
}

/// The bytes between a named global's opening brace and its matching close.
///
/// The writer indents nested tables but always puts a lone `}` in the first
/// column to close a top-level global, which is what this looks for. That is a
/// property of the file format rather than of Lua, which is precisely why this
/// is not trying to be a parser.
fn global_body<'a>(bytes: &'a [u8], marker: &[u8]) -> Option<&'a [u8]> {
    let start = find(bytes, marker)? + marker.len();
    let rest = &bytes[start..];
    let end = find(rest, b"\n}")?;
    Some(&rest[..end])
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Every `["key"] = "value"` entry in a global's body, values still escaped.
fn string_entries(body: &[u8]) -> Result<Vec<(String, Vec<u8>)>, PriceError> {
    let mut found = Vec::new();
    let mut at = 0;

    while let Some(offset) = find(&body[at..], b"[\"") {
        let key_start = at + offset + 2;
        let Some(key_len) = find(&body[key_start..], b"\"]") else {
            break;
        };
        let key = String::from_utf8_lossy(&body[key_start..key_start + key_len]).into_owned();
        let after_key = key_start + key_len + 2;

        // Only a value that is itself a string literal interests us. A number
        // (`__dbversion`) or a table is skipped without complaint.
        let Some(eq) = body[after_key..]
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
        else {
            break;
        };
        let value_at = after_key + eq;
        if body.get(value_at) != Some(&b'=') {
            at = after_key;
            continue;
        }
        let Some(quote) = body[value_at + 1..]
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
        else {
            break;
        };
        let value_start = value_at + 1 + quote;
        if body.get(value_start) != Some(&b'"') {
            at = value_start;
            continue;
        }

        let end = closing_quote(body, value_start + 1)
            .ok_or_else(|| PriceError::UnterminatedString(key.clone()))?;
        found.push((key, body[value_start + 1..end].to_vec()));
        at = end + 1;
    }

    Ok(found)
}

/// Every `["123"] = 456,` entry in a global's body.
fn integer_entries(body: &[u8]) -> BTreeMap<u32, u64> {
    let mut found = BTreeMap::new();
    for line in body.split(|byte| *byte == b'\n') {
        let text = String::from_utf8_lossy(line);
        let text = text.trim();
        let Some(rest) = text.strip_prefix("[\"") else {
            continue;
        };
        let Some((key, value)) = rest.split_once("\"] = ") else {
            continue;
        };
        let value = value.trim_end_matches(',').trim();
        if let (Ok(id), Ok(price)) = (key.parse::<u32>(), value.parse::<u64>()) {
            found.insert(id, price);
        }
    }
    found
}

/// The index of the quote that ends a string literal starting at `from`.
///
/// A quote preceded by an odd number of backslashes is escaped and does not
/// end anything. Counting rather than toggling matters: `\\"` ends the string
/// and `\"` does not.
fn closing_quote(body: &[u8], from: usize) -> Option<usize> {
    let mut index = from;
    while index < body.len() {
        if body[index] == b'"' {
            let mut backslashes = 0;
            let mut back = index;
            while back > from && body[back - 1] == b'\\' {
                backslashes += 1;
                back -= 1;
            }
            if backslashes % 2 == 0 {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

/// Turn a Lua string literal's bytes back into the bytes it stood for.
///
/// The game's SavedVariables writer escapes exactly five things and writes
/// every other byte raw, control characters included. Observed across a 5 MB
/// real file: `\n`, `\r`, `\"`, `\\`, and NUL as a decimal `\000`. Decimal
/// escapes of one to three digits are handled generally rather than just the
/// NUL case, because that is what Lua's own syntax allows and the cost of
/// being right is three lines.
fn unescape(literal: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(literal.len());
    let mut index = 0;
    while index < literal.len() {
        if literal[index] != b'\\' {
            out.push(literal[index]);
            index += 1;
            continue;
        }
        match literal.get(index + 1) {
            Some(b'n') => {
                out.push(b'\n');
                index += 2;
            }
            Some(b'r') => {
                out.push(b'\r');
                index += 2;
            }
            Some(b't') => {
                out.push(b'\t');
                index += 2;
            }
            Some(b'"') => {
                out.push(b'"');
                index += 2;
            }
            Some(b'\'') => {
                out.push(b'\'');
                index += 2;
            }
            Some(b'\\') => {
                out.push(b'\\');
                index += 2;
            }
            Some(byte) if byte.is_ascii_digit() => {
                let mut value: u32 = 0;
                let mut digits = 0;
                while digits < 3 {
                    match literal.get(index + 1 + digits) {
                        Some(digit) if digit.is_ascii_digit() => {
                            value = value * 10 + u32::from(digit - b'0');
                            digits += 1;
                        }
                        _ => break,
                    }
                }
                // Lua allows \256 through \999 to be written; they cannot fit
                // in a byte. Dropping is wrong and panicking is worse, so this
                // keeps the low byte and moves on, which is what the value
                // would have been had it round-tripped.
                out.push((value & 0xff) as u8);
                index += 1 + digits;
            }
            // A trailing backslash, or an escape the writer does not emit.
            // Keeping it literal loses nothing we had.
            _ => {
                out.push(b'\\');
                index += 1;
            }
        }
    }
    out
}

fn decode_realm(blob: &[u8]) -> Result<RealmPrices, String> {
    let value: ciborium::Value = ciborium::from_reader(blob).map_err(|error| error.to_string())?;
    let entries = value.as_map().ok_or("the blob is not a map")?;

    let mut items = BTreeMap::new();
    for (key, value) in entries {
        let Some(key) = as_text(key) else {
            continue;
        };
        // A `version` sentinel sits among the items as a bare number.
        let Some(fields) = value.as_map() else {
            continue;
        };
        let mut prices = ItemPrices::default();
        for (field, value) in fields {
            match as_text(field).as_deref() {
                Some("m") => prices.last_seen_min = as_u64(value),
                Some("h") => prices.high = day_map(value),
                Some("l") => prices.low = day_map(value),
                Some("a") => prices.available = day_map(value),
                _ => {}
            }
        }
        items.insert(key, prices);
    }

    Ok(RealmPrices { items })
}

/// CBOR carries Auctionator's keys as byte strings, not text strings, because
/// that is what LibCBOR does with Lua strings. Accept either rather than
/// depending on which one a future version picks.
fn as_text(value: &ciborium::Value) -> Option<String> {
    match value {
        ciborium::Value::Text(text) => Some(text.clone()),
        ciborium::Value::Bytes(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}

fn as_u64(value: &ciborium::Value) -> Option<u64> {
    match value {
        ciborium::Value::Integer(integer) => u128::try_from(*integer)
            .ok()
            .and_then(|v| u64::try_from(v).ok()),
        // Lua has one number type, so a value that happens to be integral can
        // still arrive as a float. Prices are whole copper either way.
        ciborium::Value::Float(float) if float.is_finite() && *float >= 0.0 => Some(*float as u64),
        _ => None,
    }
}

/// A day-keyed table. An empty Lua table serializes as an empty CBOR *array*,
/// not an empty map — the same `{}` ambiguity the collector schema already
/// has to live with, arriving here by a different road.
fn day_map(value: &ciborium::Value) -> BTreeMap<i64, u64> {
    let mut found = BTreeMap::new();
    let Some(entries) = value.as_map() else {
        return found;
    };
    for (day, price) in entries {
        let Some(day) = as_text(day).and_then(|text| text.parse::<i64>().ok()) else {
            continue;
        };
        if let Some(price) = as_u64(price) {
            found.insert(day, price);
        }
    }
    found
}

/// Auctionator's realm key for a character, or `None` when either half is
/// missing. Both halves are needed: prices are per faction, and valuing Horde
/// holdings at Alliance prices would be quietly wrong rather than obviously so.
pub fn realm_key(realm: Option<&str>, faction: Option<&str>) -> Option<String> {
    Some(format!("{} {}", realm?, faction?))
}
