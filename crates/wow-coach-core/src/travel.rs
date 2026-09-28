//! Who on the roster can get you where.
//!
//! This is the half of the teleport question that is a rule rather than a
//! fact. The collector reports the spell ids a character knows and nothing
//! else; deciding that id 3567 means Orgrimmar, that Orgrimmar is a Horde
//! city, and that a portal serves a group where a teleport serves one person,
//! all happens here. Putting that table in the addon would have been fewer
//! lines and would have frozen a rule into the data, needing an addon update
//! and a recapture every time it was wrong.
//!
//! # Why this exists at all
//!
//! Crafting can now say "the reagents are on your bank alt, in its bank". That
//! is a errand, and the length of the errand is the part the player actually
//! weighs. A character who can teleport to the city its bank is in turns a
//! flight path and ten minutes into a cast.
//!
//! # What it deliberately does not do
//!
//! It does not infer from level. A level 37 mage is *eligible* for four
//! teleports and may have trained none of them — the trainer is a separate
//! trip, and one of those spells is in a different city. Before the spellbook
//! was captured, level was the only signal available and it answered a
//! different question than the one asked. Nothing here falls back to it.
//!
//! The table below is Classic and Burning Crusade. Every id was checked
//! against its spell page rather than written from memory, because a wrong id
//! does not fail loudly: it reports the wrong destination, which is worse than
//! reporting none.

use std::collections::BTreeMap;

use crate::roster::RosterEntry;

/// Which side of the world a destination is on.
///
/// Not cosmetic: sending an Alliance character to Orgrimmar is not a slow
/// route, it is a death. A destination its faction cannot use is not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    Alliance,
    Horde,
    /// Reachable by both. Shattrath is the only one in this era.
    Neutral,
}

impl Side {
    /// Whether a character of `faction` can use a destination on this side.
    ///
    /// An unknown faction gets `true`: the capture not saying which side a
    /// character is on is a gap in the data, and hiding a real option because
    /// of it would be the wrong way to be cautious.
    pub fn usable_by(&self, faction: Option<&str>) -> bool {
        match (self, faction) {
            (Side::Neutral, _) | (_, None) => true,
            (Side::Alliance, Some(f)) => f.eq_ignore_ascii_case("Alliance"),
            (Side::Horde, Some(f)) => f.eq_ignore_ascii_case("Horde"),
        }
    }
}

/// A teleport moves the caster. A portal moves the group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Teleport,
    Portal,
}

impl Kind {
    pub fn label(&self) -> &'static str {
        match self {
            Kind::Teleport => "teleport",
            Kind::Portal => "portal",
        }
    }
}

/// One travel spell: what it is, and where it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TravelSpell {
    pub spell_id: u32,
    pub destination: &'static str,
    pub side: Side,
    pub kind: Kind,
}

/// Every mage teleport and portal in the Classic and Burning Crusade clients.
///
/// Ids verified individually against their spell pages. The order is by id so
/// a duplicate is visible on sight, and a test asserts there are none.
const TRAVEL_SPELLS: &[TravelSpell] = &[
    // Teleports — one caster.
    t(3561, "Stormwind", Side::Alliance),
    t(3562, "Ironforge", Side::Alliance),
    t(3563, "Undercity", Side::Horde),
    t(3565, "Darnassus", Side::Alliance),
    t(3566, "Thunder Bluff", Side::Horde),
    t(3567, "Orgrimmar", Side::Horde),
    t(32271, "Exodar", Side::Alliance),
    t(32272, "Silvermoon", Side::Horde),
    t(33690, "Shattrath", Side::Neutral),
    // Portals — the whole group.
    p(10059, "Stormwind", Side::Alliance),
    p(11416, "Ironforge", Side::Alliance),
    p(11417, "Orgrimmar", Side::Horde),
    p(11418, "Undercity", Side::Horde),
    p(11419, "Darnassus", Side::Alliance),
    p(11420, "Thunder Bluff", Side::Horde),
    p(32266, "Exodar", Side::Alliance),
    p(32267, "Silvermoon", Side::Horde),
    p(33691, "Shattrath", Side::Neutral),
];

const fn t(spell_id: u32, destination: &'static str, side: Side) -> TravelSpell {
    TravelSpell {
        spell_id,
        destination,
        side,
        kind: Kind::Teleport,
    }
}

const fn p(spell_id: u32, destination: &'static str, side: Side) -> TravelSpell {
    TravelSpell {
        spell_id,
        destination,
        side,
        kind: Kind::Portal,
    }
}

/// The travel spell with this id, if it is one.
pub fn spell(spell_id: u32) -> Option<&'static TravelSpell> {
    TRAVEL_SPELLS
        .iter()
        .find(|entry| entry.spell_id == spell_id)
}

/// Everything in the table, for a reader that wants to show the whole set.
pub fn all() -> &'static [TravelSpell] {
    TRAVEL_SPELLS
}

/// What one character can do.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterTravel {
    pub name: String,
    pub faction: Option<String>,
    /// Sorted by destination, then teleports before portals.
    pub spells: Vec<TravelSpell>,
    /// True when the collector has never captured this character's spellbook.
    ///
    /// The distinction this preserves: a character with no travel spells and
    /// one that has never been looked at both show an empty list, and only one
    /// of them justifies saying "this character cannot get you there".
    pub spellbook_seen: bool,
}

impl CharacterTravel {
    pub fn reaches(&self, destination: &str) -> bool {
        self.spells
            .iter()
            .any(|spell| spell.destination.eq_ignore_ascii_case(destination))
    }
}

/// The whole roster's travel options.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TravelPlan {
    pub characters: Vec<CharacterTravel>,
    /// Destination to the characters who can reach it, best first: a portal
    /// before a teleport, because a portal also carries whoever is with them.
    pub by_destination: BTreeMap<String, Vec<String>>,
    pub caveats: Vec<String>,
}

/// Anything that looks like a travel spell by name but is not in the table.
///
/// A deliberate, narrow exception to "never match on names". It is never used
/// to answer a question — only to say that the table may be short, so an id
/// this build predates shows up as a gap rather than as a silent absence. It
/// only works in English, which is exactly why it reports rather than decides.
fn unrecognised_travel_spells(entries: &[RosterEntry<'_>]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for entry in entries {
        for known in entry.record.spells.iter().flatten() {
            let (Some(id), Some(name)) = (known.spell_id, known.name.as_deref()) else {
                continue;
            };
            if spell(id).is_some() {
                continue;
            }
            if name.starts_with("Teleport:") || name.starts_with("Portal:") {
                found.push(format!("{name} ({id})"));
            }
        }
    }
    found.sort_unstable();
    found.dedup();
    found
}

/// Build the travel view for a roster.
pub fn plan(entries: &[RosterEntry<'_>]) -> TravelPlan {
    let mut plan = TravelPlan::default();
    let mut by_destination: BTreeMap<String, Vec<(Kind, String)>> = BTreeMap::new();
    let mut unseen = Vec::new();

    for entry in entries {
        let name = entry.name().to_string();
        let faction = entry.record.faction.clone();
        let spellbook_seen = entry.record.spells.is_some();
        if !spellbook_seen {
            unseen.push(name.clone());
        }

        let mut spells: Vec<TravelSpell> = entry
            .record
            .spells
            .iter()
            .flatten()
            .filter_map(|known| known.spell_id)
            .filter_map(spell)
            .copied()
            // A character holding a spell for the other side would be a bug in
            // the capture, not an option to offer.
            .filter(|found| found.side.usable_by(faction.as_deref()))
            .collect();
        spells.sort_by(|a, b| {
            a.destination
                .cmp(b.destination)
                .then_with(|| a.kind.cmp(&b.kind))
        });
        spells.dedup();

        for found in &spells {
            by_destination
                .entry(found.destination.to_string())
                .or_default()
                .push((found.kind, name.clone()));
        }

        plan.characters.push(CharacterTravel {
            name,
            faction,
            spells,
            spellbook_seen,
        });
    }

    plan.characters.sort_by(|a, b| {
        b.spells
            .len()
            .cmp(&a.spells.len())
            .then_with(|| a.name.cmp(&b.name))
    });

    for (destination, mut holders) in by_destination {
        // A portal first: it takes the group, so it answers strictly more than
        // a teleport to the same place.
        holders.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let mut names: Vec<String> = holders
            .into_iter()
            .map(|(kind, name)| match kind {
                Kind::Portal => format!("{name} (portal)"),
                Kind::Teleport => name,
            })
            .collect();
        names.dedup();
        plan.by_destination.insert(destination, names);
    }

    if !unseen.is_empty() {
        plan.caveats.push(format!(
            "No spellbook captured for {}. Log each in once with the current addon — until then \"cannot get there\" and \"never looked\" are the same blank.",
            unseen.join(", ")
        ));
    }

    let unrecognised = unrecognised_travel_spells(entries);
    if !unrecognised.is_empty() {
        plan.caveats.push(format!(
            "Known but not in this build's travel table: {}. The id is the fact, so these are real spells this version does not recognise rather than anything wrong with the capture.",
            unrecognised.join(", ")
        ));
    }

    plan
}
