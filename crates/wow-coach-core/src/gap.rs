//! Knowing which characters exist but have not been captured yet.
//!
//! The collector only records the character you are logged in as, so a roster
//! fills up one login at a time. Until it is full, any advice about *which*
//! character to play is being given while blind to the rest — and a confident
//! recommendation drawn from three of eighteen characters is worse than an
//! honest one that says what it cannot see.
//!
//! The game itself keeps a folder per character ever logged in, so the full
//! roster can be learned without any addon at all. Reading those folders is
//! the caller's job: this module is pure, because the core compiles to
//! `wasm32` where there is no filesystem. The CLI and desktop clients walk
//! the disk; the browser client gets the same list from whatever the player
//! hands it. Both then ask the same question here.

use std::collections::BTreeMap;

use crate::collector::CharacterRecord;

/// A character the game knows about, learned from somewhere other than the
/// collector — normally a folder name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownCharacter {
    /// The flavor directory this was found under, e.g. `_anniversary_`. Kept
    /// as found rather than mapped to a flavor name, because the mapping is
    /// the reader's guess and this is meant to be evidence.
    pub flavor_dir: Option<String>,
    pub realm: String,
    pub name: String,
}

/// What the collector has, against what the game says exists.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RosterGap {
    /// Characters with captured data.
    pub captured: Vec<String>,
    /// Characters the game knows about that have never been captured.
    pub unseen: Vec<KnownCharacter>,
}

impl RosterGap {
    pub fn total(&self) -> usize {
        self.captured.len() + self.unseen.len()
    }

    pub fn is_complete(&self) -> bool {
        self.unseen.is_empty()
    }

    /// A sentence for the reader, or nothing when there is no gap to report.
    pub fn summary(&self) -> Option<String> {
        if self.is_complete() {
            return None;
        }
        let names: Vec<&str> = self
            .unseen
            .iter()
            .map(|character| character.name.as_str())
            .collect();
        Some(format!(
            "Showing {} of {} characters. Not yet captured: {}. \
             Log in on each once, then log out, to include it.",
            self.captured.len(),
            self.total(),
            names.join(", ")
        ))
    }
}

fn same(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

/// Remove folder names that shadow a plainer one in the same flavor.
///
/// The Forever client writes two trees: the real character folder sits under a
/// numeric realm with a name like `Gravehexx-Gravehex`, while the folder whose
/// name matches what the game's API actually reports — `Gravehexx` — is a stub
/// holding only an addon list. The suffix's meaning is not established, so
/// rather than guess at it, a hyphenated name is dropped when its leading part
/// is also present. That keeps the name the collector will write and discards
/// the duplicate, without pretending to understand the layout.
fn drop_shadowed(mut known: Vec<KnownCharacter>) -> Vec<KnownCharacter> {
    let plain: Vec<(Option<String>, String)> = known
        .iter()
        .map(|character| (character.flavor_dir.clone(), character.name.clone()))
        .collect();

    known.retain(|character| {
        let Some((base, _)) = character.name.split_once('-') else {
            return true;
        };
        // Keep it unless a plainer name in the same flavor already covers it.
        !plain.iter().any(|(flavor, name)| {
            flavor == &character.flavor_dir && same(name, base) && name != &character.name
        })
    });
    known
}

/// Compare what the game knows against what the collector captured.
pub fn find_gap(
    known: &[KnownCharacter],
    characters: &BTreeMap<String, CharacterRecord>,
) -> RosterGap {
    let mut captured: Vec<String> = characters
        .iter()
        .map(|(key, record)| record.display_name(key).to_string())
        .collect();
    captured.sort();
    captured.dedup();

    let mut unseen: Vec<KnownCharacter> = drop_shadowed(known.to_vec())
        .into_iter()
        .filter(|candidate| {
            !characters.values().any(|record| {
                let name_matches = record
                    .name
                    .as_deref()
                    .is_some_and(|name| same(name, &candidate.name));
                if !name_matches {
                    return false;
                }
                // Realm confirms the match where both sides know it. A record
                // with no realm still matches on name, because a wrong match
                // here only hides a character from the gap list, while a
                // missed match invents one that does not exist.
                match record.realm.as_deref() {
                    Some(realm) if !candidate.realm.is_empty() => same(realm, &candidate.realm),
                    _ => true,
                }
            })
        })
        .collect();

    unseen.sort_by(|a, b| a.name.cmp(&b.name));
    unseen.dedup_by(|a, b| same(&a.name, &b.name) && same(&a.realm, &b.realm));

    RosterGap { captured, unseen }
}
