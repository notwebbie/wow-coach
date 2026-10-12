//! Character roles, and what they do and do not affect.
//!
//! A role is **user configuration, not game data**. Nothing in SavedVariables
//! can tell you that a character is a bank alt, so this never lives in the
//! collector's schema — it is stated by the player and stored beside the
//! history.
//!
//! The important design point: a role decides which rules apply to a
//! character, not whether it appears. A bank alt leaves the play rotation but
//! keeps its professions, its bags and its gold, because that is the entire
//! reason it exists. The earlier prototype dropped bank characters from the
//! dashboard altogether, which hid exactly the inventory the player was
//! keeping them for.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::collector::CharacterRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Seen, but the player has not said what it is for.
    ///
    /// The default, and deliberately **not** in the play rotation. Rolling a
    /// character to hold a name should not quietly enter it into "what should
    /// I play next" the first time the addon sees it: the player never asked
    /// for that, and the ranking it pollutes is the tool's main output.
    ///
    /// This was `Active` until the levelling rotation was made a choice.
    /// Missing from a ranking is a mild, visible problem — the roster lists
    /// the character and says it is unset. Wrongly present in one is an
    /// invisible wrong answer.
    #[default]
    Unclassified,
    /// Being levelled now. In the rotation and in every view.
    Active,
    /// Storage and crafting. Out of the play rotation; still counted for
    /// professions, materials and gold.
    Bank,
    /// Was being played and has been set down. Out of the rotation and not
    /// nagged about, but still visible.
    Parked,
    /// Made to hold a name or a look, and never started.
    ///
    /// Behaviourally close to `Parked`, and separate for one reason that is
    /// not cosmetic: a parked character has progress and a rested bar it was
    /// accruing on purpose, so that bar filling up is worth a word. A
    /// name-holder has nothing to waste. Collapsing the two means either
    /// nagging about a level 1 nobody intends to play, or going quiet about a
    /// character that is wasting the rest it banked.
    Reserved,
}

impl Role {
    /// Whether "what should I play next" should consider this character.
    pub fn in_play_rotation(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Whether professions, materials and gold should count this character.
    /// Every role does — a bank alt is mostly economy.
    pub fn in_economy(&self) -> bool {
        true
    }

    /// Whether to raise reminders (unspent talents, full quest log, trainer).
    /// Pointless for a character the player has deliberately set down.
    pub fn wants_reminders(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Whether rested XP going to waste is worth mentioning.
    ///
    /// The one behaviour separating `Parked` from `Reserved`, and the reason
    /// the two are not one role.
    pub fn wants_rested_warnings(&self) -> bool {
        matches!(self, Self::Active | Self::Parked)
    }

    /// Whether the player has said what this character is for.
    pub fn is_decided(&self) -> bool {
        !matches!(self, Self::Unclassified)
    }

    /// The word the player types and reads.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Unclassified => "unset",
            Self::Active => "active",
            Self::Bank => "bank",
            Self::Parked => "parked",
            Self::Reserved => "reserved",
        }
    }

    /// Parse a role the player typed, including the words they actually use
    /// for it. "levelling" is what this list is for, whatever the enum calls
    /// it.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "a" | "active" | "levelling" | "leveling" => Some(Self::Active),
            "b" | "bank" => Some(Self::Bank),
            "p" | "park" | "parked" => Some(Self::Parked),
            "r" | "reserve" | "reserved" => Some(Self::Reserved),
            _ => None,
        }
    }
}

/// Player-stated configuration, keyed by the collector's character key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RosterConfig {
    pub roles: BTreeMap<String, Role>,
    /// Explicit play order. Empty means no preference, and the engine ranks by
    /// its own rules rather than by a list — which is what the prototype
    /// silently fell back to while still claiming a configured rotation.
    pub rotation: Vec<String>,
    /// The talent tree each character is being built in, as the game names
    /// it. Player-stated: points spent can suggest a spec, but a character
    /// mid-respec or with one stray point would be guessed wrong.
    pub specs: BTreeMap<String, String>,
}

impl RosterConfig {
    pub fn role_for(&self, key: &str) -> Role {
        self.roles.get(key).copied().unwrap_or_default()
    }

    pub fn spec_for(&self, key: &str) -> Option<&str> {
        self.specs.get(key).map(String::as_str)
    }

    pub fn set_spec(&mut self, key: impl Into<String>, tree: impl Into<String>) {
        self.specs.insert(key.into(), tree.into());
    }

    pub fn clear_spec(&mut self, key: &str) -> bool {
        self.specs.remove(key).is_some()
    }

    pub fn set_role(&mut self, key: impl Into<String>, role: Role) {
        self.roles.insert(key.into(), role);
    }

    /// Forget a decision, returning the character to unclassified.
    ///
    /// This is not the same as setting it active. An explicit role — even
    /// `active` — means the player has decided, so suggestions stop for that
    /// character forever. Undoing has to remove the entry rather than
    /// overwrite it, or there is no way back from a mistaken choice.
    pub fn clear_role(&mut self, key: &str) -> bool {
        self.roles.remove(key).is_some()
    }

    /// Whether the player has decided about this character at all.
    ///
    /// Reads the stored role rather than merely the key's presence, so a
    /// config that spells out `unclassified` means the same as one that leaves
    /// the character out. Two ways to say "undecided" that disagreed would be
    /// a trap for anyone hand-editing this file.
    pub fn is_classified(&self, key: &str) -> bool {
        self.roles.get(key).is_some_and(Role::is_decided)
    }

    /// Resolve a player-typed name to a character key, so configuration can be
    /// written in terms of names while staying keyed by identity. Matching is
    /// case-insensitive because players do not type their own capitalisation
    /// consistently.
    pub fn resolve<'a>(
        characters: &'a BTreeMap<String, CharacterRecord>,
        name: &str,
    ) -> Option<&'a str> {
        characters
            .iter()
            .find(|(_, record)| {
                record
                    .name
                    .as_deref()
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
            })
            .map(|(key, _)| key.as_str())
    }
}

/// A character paired with its role, which is how the rest of the engine wants
/// to see it.
#[derive(Debug, Clone, PartialEq)]
pub struct RosterEntry<'a> {
    pub key: &'a str,
    pub record: &'a CharacterRecord,
    pub role: Role,
}

impl<'a> RosterEntry<'a> {
    pub fn name(&self) -> &'a str {
        self.record.display_name(self.key)
    }
}

/// Pair every character with its role.
pub fn roster<'a>(
    characters: &'a BTreeMap<String, CharacterRecord>,
    config: &RosterConfig,
) -> Vec<RosterEntry<'a>> {
    characters
        .iter()
        .map(|(key, record)| RosterEntry {
            key,
            record,
            role: config.role_for(key),
        })
        .collect()
}
