//! What the whole roster makes and holds.
//!
//! This is the view a per-character window cannot give you. Professions are
//! spread across alts on purpose, materials pile up wherever they dropped, and
//! the character holding the ore is rarely the one who can smelt it. Answering
//! "who can make this" or "what am I sitting on" means looking at every
//! character at once.
//!
//! **Every role counts here.** That is the entire point of marking a bank alt:
//! it leaves the play rotation and keeps its professions, its bags and its
//! gold. A character deliberately set aside is still holding the herbs. The
//! prototype dropped non-active characters from its dashboard and so hid
//! exactly the inventory they were being kept for.
//!
//! # What a valuation here is, and is not
//!
//! Prices come from Auctionator's record of the last minimum buyout it saw in
//! a scan. That is a real observation, not a projection, and it is not what
//! the player would receive: it ignores the auction house's cut, assumes the
//! stock would actually sell, and is only as fresh as the last scan. Every
//! figure therefore travels with how stale it is, and anything without a price
//! is reported as unpriced rather than counted as worthless.
//!
//! One property of that data shapes everything built on it. Auctionator's
//! daily figure is the best minimum buyout it happened to *see*, so a day the
//! player ran a targeted search records a much smaller number than a day they
//! ran a full scan. The day-to-day series therefore measures scan completeness
//! as much as it measures price, and nothing here may treat a rise between two
//! days as a market move. It is only safe to compare across days at magnitudes
//! far beyond what that artifact reaches — which is the whole reason the
//! outlier threshold below is set where it is.

use std::collections::BTreeMap;

use crate::auctionator::{self, PriceDb, RealmPrices};
use crate::roster::{Role, RosterEntry};

/// A profession somebody on the roster has.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfessionHolder {
    pub key: String,
    pub name: String,
    pub role: Role,
    pub profession: String,
    pub rank: Option<u16>,
    pub max_rank: Option<u16>,
    pub recipes_known: Option<usize>,
}

impl ProfessionHolder {
    /// True when the skill is at its cap and needs a trainer to go further.
    pub fn needs_training(&self) -> bool {
        matches!((self.rank, self.max_rank), (Some(rank), Some(max)) if max > 0 && rank >= max)
    }
}

/// One item in one market, totalled across everyone there holding it.
///
/// **Market, not item, is the unit.** Horde and Alliance run separate auction
/// houses on the same realm and the same item routinely differs by more than
/// double between them. Pooling a cross-faction roster into one row would
/// price half of it at the wrong market — quietly, and in the direction
/// nobody would check. Found in live data: three characters holding one item
/// across two factions, valued as a single pile.
#[derive(Debug, Clone, PartialEq)]
pub struct Holding {
    /// Auctionator's realm key, e.g. `"Dreamscythe Horde"`. Absent when the
    /// capture did not say which realm or faction the holder was on.
    pub market: Option<String>,
    pub item_id: u32,
    /// The client's own name for it, when the capture had one. Never a key.
    pub name: Option<String>,
    pub count: u32,
    /// Character name to the count it holds, so the player knows where to go.
    pub held_by: BTreeMap<String, u32>,
    /// Last seen minimum buyout per unit, in copper.
    pub unit_price: Option<u64>,
    /// The middle of the recent daily prices, which one odd listing cannot
    /// move. Absent when there is no history to take a middle of.
    pub typical_price: Option<u64>,
    /// `unit_price * count`, absent when the price is.
    pub value: Option<u64>,
    /// `typical_price * count`, for the same reason.
    pub typical_value: Option<u64>,
    /// How many days before the realm's most recent scan this item was last
    /// seen. Zero means it was in the latest scan.
    pub days_stale: Option<i64>,
}

/// How far out of line a last-seen price has to be before it is called out.
///
/// This threshold is high on purpose, and the reason is a property of the data
/// rather than of markets. Auctionator's daily `h` is the highest *minimum*
/// buyout it saw that day, and what it saw depends entirely on how complete
/// that day's scan was. A day when the player ran a targeted search records a
/// small figure; a day they ran a full scan records the real one. Comparing
/// across days therefore measures scan completeness as much as it measures
/// price.
///
/// Measured on a real four-day database: that artifact alone reached about 30x
/// (one item read 29 copper on a partial day and 1,999 on a full one), while
/// the genuine mis-listing that prompted this — a Copper Rod at 1,399 gold
/// against a usual 20 to 30 silver — reached about 5,600x. A hundred sits in
/// the gap between them.
///
/// The honest caveat: that gap is drawn from one account's database and a
/// single confirmed bad listing. The threshold is deliberately set to miss
/// borderline cases rather than to flag good ones, because a marker that fires
/// on a third of the rows teaches the reader to ignore it, and then it catches
/// nothing at all. Worth revisiting against more databases.
const OUTLIER_RATIO: f64 = 100.0;

impl Holding {
    /// True when the last seen price is wildly out of line with the history.
    ///
    /// This does not mean the price is wrong. It means one listing is carrying
    /// the whole figure, and a reader deciding what to sell should know that
    /// before acting on it.
    pub fn price_looks_like_an_outlier(&self) -> bool {
        match (self.unit_price, self.typical_price) {
            (Some(last), Some(typical)) if typical > 0 && last > 0 => {
                let ratio = last as f64 / typical as f64;
                ratio >= OUTLIER_RATIO || ratio <= 1.0 / OUTLIER_RATIO
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Economy {
    /// Sorted by profession, then by rank descending.
    pub professions: Vec<ProfessionHolder>,
    /// Priced holdings, most valuable first.
    pub holdings: Vec<Holding>,
    /// Held, but Auctionator has never seen it on the auction house. Kept
    /// apart rather than valued at zero: "no price" and "worthless" are
    /// different claims and only one of them is supported.
    pub unpriced: Vec<Holding>,
    /// Coin carried by every character counted here.
    pub gold_copper: u64,
    /// The sum of `holdings` at last seen prices.
    pub holdings_value: u64,
    /// The same with the flagged outliers valued at their typical price
    /// instead. Only those: substituting everywhere would swap a market
    /// figure for an artifact of how complete each day's scan happened to be.
    pub holdings_value_typical: u64,
    pub caveats: Vec<String>,
}

impl Economy {
    /// Everyone who has this profession, best first.
    pub fn who_has(&self, profession: &str) -> Vec<&ProfessionHolder> {
        let mut found: Vec<&ProfessionHolder> = self
            .professions
            .iter()
            .filter(|holder| holder.profession.eq_ignore_ascii_case(profession))
            .collect();
        found.sort_by_key(|holder| std::cmp::Reverse(holder.rank.unwrap_or(0)));
        found
    }
}

/// Build the roster-wide view.
///
/// `prices` may be empty: the professions and holdings are still worth seeing
/// without a valuation, and saying so beats refusing to answer.
pub fn survey(entries: &[RosterEntry<'_>], prices: &PriceDb) -> Economy {
    let mut economy = Economy::default();
    // Keyed by market first: see `Holding`.
    let mut stacks: BTreeMap<(Option<String>, u32), BTreeMap<String, u32>> = BTreeMap::new();
    let mut item_names: BTreeMap<u32, String> = BTreeMap::new();
    let mut realms_used: BTreeMap<String, Option<&RealmPrices>> = BTreeMap::new();
    let mut counted = 0usize;
    let mut bags_missing = Vec::new();

    for entry in entries {
        if !entry.role.in_economy() {
            continue;
        }
        counted += 1;
        let record = entry.record;
        let name = entry.name().to_string();

        economy.gold_copper = economy
            .gold_copper
            .saturating_add(record.money_copper.unwrap_or(0));

        // Professions come from the recipe cache, not from the skill list.
        // The client's skill list also holds talent tabs, weapon skills,
        // armour proficiencies and languages, none of which are professions
        // and all of which sit permanently at cap. The recipe cache only ever
        // holds skills read out of a trade skill window, which makes it a
        // trade skill by construction — and locale-independent, which a list
        // of profession names would not be.
        for (profession, cache) in record.recipes.iter().flatten() {
            economy.professions.push(ProfessionHolder {
                key: entry.key.to_string(),
                name: name.clone(),
                role: entry.role,
                profession: profession.clone(),
                rank: cache.rank,
                max_rank: cache.max_rank,
                recipes_known: cache.recipes.as_ref().map(Vec::len),
            });
        }

        let realm_key = auctionator::realm_key(record.realm.as_deref(), record.faction.as_deref());
        if let Some(key) = &realm_key {
            realms_used
                .entry(key.clone())
                .or_insert_with(|| prices.realms.get(key));
        }

        match record
            .inventory
            .as_ref()
            .and_then(|bags| bags.contents.as_ref())
        {
            Some(contents) => {
                for stack in contents {
                    let (Some(item_id), Some(count)) = (stack.item_id, stack.count) else {
                        continue;
                    };
                    *stacks
                        .entry((realm_key.clone(), item_id))
                        .or_default()
                        .entry(name.clone())
                        .or_insert(0) += count;
                    if let Some(item_name) = stack.name.as_deref() {
                        // A name is display only, so the first one wins and it
                        // is shared across markets. The id remains the key.
                        item_names
                            .entry(item_id)
                            .or_insert_with(|| item_name.to_string());
                    }
                }
            }
            None => bags_missing.push(name.clone()),
        }
    }

    economy.professions.sort_by(|a, b| {
        a.profession
            .cmp(&b.profession)
            .then_with(|| b.rank.unwrap_or(0).cmp(&a.rank.unwrap_or(0)))
            .then_with(|| a.name.cmp(&b.name))
    });

    for ((market, item_id), held_by) in stacks {
        let count: u32 = held_by.values().copied().sum();
        let realm = market.as_ref().and_then(|key| prices.realms.get(key));
        let item = realm.and_then(|realm| realm.for_item(item_id));
        let unit_price = item
            .and_then(|item| item.last_seen_min)
            .filter(|price| *price > 0);
        let typical_price = item
            .and_then(|item| item.typical())
            .filter(|price| *price > 0);
        let days_stale = match (
            realm.and_then(RealmPrices::last_scan_day),
            item.and_then(|item| item.last_seen_day()),
        ) {
            (Some(latest), Some(seen)) => Some(latest - seen),
            _ => None,
        };
        let holding = Holding {
            market,
            item_id,
            name: item_names.get(&item_id).cloned(),
            count,
            held_by,
            unit_price,
            typical_price,
            value: unit_price.map(|price| price.saturating_mul(u64::from(count))),
            typical_value: typical_price.map(|price| price.saturating_mul(u64::from(count))),
            days_stale,
        };
        if holding.value.is_some() {
            economy.holdings.push(holding);
        } else {
            economy.unpriced.push(holding);
        }
    }

    economy.holdings.sort_by(|a, b| {
        b.value
            .unwrap_or(0)
            .cmp(&a.value.unwrap_or(0))
            .then_with(|| a.item_id.cmp(&b.item_id))
            .then_with(|| a.market.cmp(&b.market))
    });
    economy.unpriced.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.item_id.cmp(&b.item_id))
            .then_with(|| a.market.cmp(&b.market))
    });
    economy.holdings_value = economy
        .holdings
        .iter()
        .filter_map(|holding| holding.value)
        .fold(0u64, u64::saturating_add);
    // Every holding contributes, so the two totals cover the same stock and
    // differ only where a price was called implausible.
    economy.holdings_value_typical = economy
        .holdings
        .iter()
        .filter_map(|holding| {
            if holding.price_looks_like_an_outlier() {
                holding.typical_value.or(holding.value)
            } else {
                holding.value
            }
        })
        .fold(0u64, u64::saturating_add);

    economy.caveats = caveats(counted, &realms_used, &bags_missing, &economy);
    economy
}

fn caveats(
    counted: usize,
    realms_used: &BTreeMap<String, Option<&RealmPrices>>,
    bags_missing: &[String],
    economy: &Economy,
) -> Vec<String> {
    let mut caveats = Vec::new();

    if counted == 0 {
        caveats.push("No characters were counted.".to_string());
        return caveats;
    }

    let unpriced_realms: Vec<&str> = realms_used
        .iter()
        .filter(|(_, prices)| prices.is_none())
        .map(|(key, _)| key.as_str())
        .collect();
    if !unpriced_realms.is_empty() {
        caveats.push(format!(
            "No Auctionator scan data for {} — run an auction house scan there to value what it holds.",
            unpriced_realms.join(", ")
        ));
    }

    if !bags_missing.is_empty() {
        caveats.push(format!(
            "Bag contents were not captured for {}, so nothing they hold is counted.",
            bags_missing.join(", ")
        ));
    }

    let nameless = economy
        .holdings
        .iter()
        .chain(&economy.unpriced)
        .filter(|holding| holding.name.is_none())
        .count();
    if nameless > 0 {
        caveats.push(format!(
            "{nameless} item type(s) have no name yet — the client had not loaded them when the capture ran. Their ids are still correct."
        ));
    }

    if !economy.unpriced.is_empty() {
        caveats.push(format!(
            "{} item type(s) have never been seen on the auction house and are listed unpriced rather than counted as worthless.",
            economy.unpriced.len()
        ));
    }

    let stale: Vec<&Holding> = economy
        .holdings
        .iter()
        .filter(|holding| holding.days_stale.is_some_and(|days| days >= 7))
        .collect();
    if !stale.is_empty() {
        caveats.push(format!(
            "{} of the priced item type(s) were last seen on the auction house a week or more before your latest scan, so those prices are old.",
            stale.len()
        ));
    }

    let markets: std::collections::BTreeSet<&str> = economy
        .holdings
        .iter()
        .chain(&economy.unpriced)
        .filter_map(|holding| holding.market.as_deref())
        .collect();
    if markets.len() > 1 {
        caveats.push(format!(
            "Your roster spans {} separate auction houses ({}). The same item can differ by more than double between them, so each is counted and priced on its own.",
            markets.len(),
            markets.iter().copied().collect::<Vec<_>>().join(", ")
        ));
    }

    let outliers: Vec<&Holding> = economy
        .holdings
        .iter()
        .filter(|holding| holding.price_looks_like_an_outlier())
        .collect();
    if !outliers.is_empty() {
        caveats.push(format!(
            "{} item type(s) have a last seen price more than {OUTLIER_RATIO:.0}x away from what they usually go for, marked ? above — one odd listing is carrying that figure, not a market.",
            outliers.len()
        ));
    }

    if economy.holdings_value > 0 {
        caveats.push(
            "Values are the last minimum buyout Auctionator saw, before the auction house's cut, and assume the stock would actually sell."
                .to_string(),
        );
    }

    caveats.push(
        "Only what is in bags is counted. The collector cannot read a bank it has not been standing in front of."
            .to_string(),
    );

    caveats
}

/// Split copper into gold, silver and copper, for display.
pub fn coin(copper: u64) -> (u64, u64, u64) {
    (copper / 10_000, (copper % 10_000) / 100, copper % 100)
}
