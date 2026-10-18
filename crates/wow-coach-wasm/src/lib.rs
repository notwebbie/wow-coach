//! Browser bindings for the WoW Coach core.
//!
//! No rules live here. Everything in this file parses, reshapes or forwards,
//! because the architecture says frontends are presentation only and neither
//! re-implements a rule — and a binding layer is the easiest place for a rule
//! to accidentally grow a second home.
//!
//! See `README.md` for why this is a separate crate from the core, and for the
//! two shapes that cross the boundary.

use std::collections::BTreeMap;

use serde::Serialize;
use wasm_bindgen::prelude::*;
use wow_coach_core::coaching::{self, Advice};
use wow_coach_core::collector::{self, CharacterRecord};
use wow_coach_core::roster::{self, Role, RosterConfig};

/// One player's data, parsed once and held.
///
/// The page constructs this with the contents of their SavedVariables file and
/// then asks it questions. Parsing on every call would be simpler and would
/// re-read the whole file to answer "what are my characters called".
#[wasm_bindgen]
#[derive(Debug)]
pub struct Session {
    characters: BTreeMap<String, CharacterRecord>,
    config: RosterConfig,
    notices: Vec<String>,
}

/// The work, in plain Rust.
///
/// Separate from the `#[wasm_bindgen]` block below so it can be tested on a
/// native target. A binding layer is exactly where a wrong field name or a
/// wrong nesting reaches the page unnoticed, so it does not get to be the one
/// untested thing in the project.
impl Session {
    pub fn open(saved_variables: &str) -> Result<Session, String> {
        let loaded = collector::load(saved_variables)
            .map_err(|error| format!("could not read that file: {error}"))?;
        Ok(Session {
            characters: loaded.db.characters,
            config: RosterConfig::default(),
            notices: loaded.notices,
        })
    }

    pub fn notices_json(&self) -> Result<String, String> {
        json(&self.notices)
    }

    pub fn characters_json(&self) -> Result<String, String> {
        let views: Vec<CharacterView> = roster::roster(&self.characters, &self.config)
            .iter()
            .map(CharacterView::from)
            .collect();
        json(&views)
    }

    pub fn apply_roles(&mut self, roles_json: &str) -> Result<(), String> {
        let roles: BTreeMap<String, String> = serde_json::from_str(roles_json)
            .map_err(|error| format!("roles must be a key to role map: {error}"))?;
        // Validated in full before anything is written, so a typo in the
        // fourth entry cannot leave the first three applied.
        let mut parsed = Vec::with_capacity(roles.len());
        for (key, label) in roles {
            if !self.characters.contains_key(&key) {
                return Err(format!("no character with the key {key}"));
            }
            let role = Role::parse(&label).ok_or_else(|| format!("{label} is not a role"))?;
            parsed.push((key, role));
        }
        for (key, role) in parsed {
            self.config.roles.insert(key, role);
        }
        Ok(())
    }

    pub fn recommendation_json(&self) -> Result<String, String> {
        let entries = roster::roster(&self.characters, &self.config);
        let advice = coaching::advise(&entries, true);
        json(&RecommendationView::from(&advice))
    }
}

#[wasm_bindgen]
impl Session {
    /// Parse a `WoWCoachCollector.lua` SavedVariables file.
    ///
    /// Fails loudly on anything outside the restricted subset the addon emits.
    /// A file this cannot read means the collector has a bug, and saying so is
    /// more useful than recovering something plausible.
    #[wasm_bindgen(constructor)]
    pub fn new(saved_variables: &str) -> Result<Session, JsError> {
        Session::open(saved_variables).map_err(|error| JsError::new(&error))
    }

    /// Anything the parse wanted to say that was not fatal — a newer schema, a
    /// record missing a field. Surfaced rather than swallowed.
    #[wasm_bindgen(getter)]
    pub fn notices(&self) -> Result<String, JsError> {
        self.notices_json().map_err(|error| JsError::new(&error))
    }

    /// Every character, with the role currently set for it.
    ///
    /// This is the first screen: the moment the tool proves it read the game
    /// correctly, by showing the player their own characters.
    pub fn characters(&self) -> Result<String, JsError> {
        self.characters_json().map_err(|error| JsError::new(&error))
    }

    /// Mark which characters are being played.
    ///
    /// Takes the whole set rather than one at a time, because the first-run
    /// question is "which of these are you actually playing" and the answer
    /// arrives as a set of tick boxes. Keys not named are left alone.
    pub fn set_roles(&mut self, roles_json: &str) -> Result<(), JsError> {
        self.apply_roles(roles_json)
            .map_err(|error| JsError::new(&error))
    }

    /// What to play next, and why.
    ///
    /// One answer, not a list — a coach gives one, and a list of five with
    /// reasons is a thing people skim and close. The rest of the ranking comes
    /// back underneath for anyone who wants it.
    pub fn recommendation(&self) -> Result<String, JsError> {
        self.recommendation_json()
            .map_err(|error| JsError::new(&error))
    }
}

/// A character as the roster screen shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CharacterView {
    key: String,
    name: String,
    level: Option<u16>,
    /// Locale-independent class file name, e.g. `MAGE`. The page picks the
    /// colour and the display name; this is the identity.
    class: Option<String>,
    race: Option<String>,
    faction: Option<String>,
    realm: Option<String>,
    zone: Option<String>,
    /// `forever`, `tbc_classic`, `classic_era`. A roster can span clients.
    flavor: Option<String>,
    /// `unset`, `active`, `bank`, `parked` or `reserved`.
    role: &'static str,
    /// Unix epoch seconds of the capture, so the page can say how stale it is.
    captured_at: Option<i64>,
}

impl From<&roster::RosterEntry<'_>> for CharacterView {
    fn from(entry: &roster::RosterEntry<'_>) -> Self {
        let record = entry.record;
        CharacterView {
            key: entry.key.to_string(),
            name: entry.name().to_string(),
            level: record.level,
            class: record.class.clone(),
            race: record.race.clone(),
            faction: record.faction.clone(),
            realm: record.realm.clone(),
            zone: record.zone.clone(),
            flavor: record.game_flavor.clone(),
            role: entry.role.label(),
            captured_at: record.captured_at,
        }
    }
}

/// The one answer, plus what sits behind it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecommendationView {
    /// Absent when nothing is in the rotation yet — which is the first-run
    /// state, not an error, and the page should ask rather than apologise.
    play_next: Option<SuggestionView>,
    /// The remainder of the ranking, best first.
    then: Vec<SuggestionView>,
    /// Resting nowhere, so accruing nothing. Parking them is the fix.
    park: Vec<SuggestionView>,
    caveats: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SuggestionView {
    key: String,
    name: String,
    level: Option<u16>,
    /// The finished sentence, built in the core so every client says the same
    /// thing. The page displays it; it does not assemble it.
    headline: String,
}

impl From<&coaching::Suggestion> for SuggestionView {
    fn from(suggestion: &coaching::Suggestion) -> Self {
        SuggestionView {
            key: suggestion.key.clone(),
            name: suggestion.name.clone(),
            level: suggestion.level,
            headline: suggestion.headline(),
        }
    }
}

impl From<&Advice> for RecommendationView {
    fn from(advice: &Advice) -> Self {
        let mut play = advice.play.iter().map(SuggestionView::from);
        RecommendationView {
            play_next: play.next(),
            then: play.collect(),
            park: advice.park.iter().map(SuggestionView::from).collect(),
            caveats: advice.caveats.clone(),
        }
    }
}

fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}
