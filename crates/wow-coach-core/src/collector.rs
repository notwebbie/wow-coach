//! Typed models for the collector's SavedVariables schema.
//!
//! The contract is in `docs/COLLECTOR-SCHEMA.md`. Two rules from it shape
//! everything here:
//!
//! - **Every field is optional.** An absent field means "not captured" — the
//!   API was unavailable, or the client never reached the state that fills it.
//!   It never means zero or false. So every field is an `Option`, including
//!   ones that look mandatory, and a record that loses a field between captures
//!   is normal rather than corrupt.
//! - **A newer schema version is read, not rejected.** A player who updates the
//!   addon before this reader should still see their characters.

use std::collections::BTreeMap;

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use crate::lua::{self, LuaValue};

/// The schema version this reader was written against. Files declaring a newer
/// version are still read, for the fields that are recognised.
pub const KNOWN_SCHEMA_VERSION: u16 = 2;

/// The global the addon writes.
pub const SAVED_VARIABLE: &str = "WoWCoachCollectorDB";

/// Accept an empty Lua table where a list is expected.
///
/// `{}` in Lua is genuinely ambiguous: it is both the empty list and the empty
/// record, and nothing in the text says which. The converter has to pick one,
/// and picks record — so a list field that happens to be empty, like the bag
/// contents of a character whose bags are empty, arrives as `{}` rather than
/// `[]`. That is not a malformed file and must not be read as one.
///
/// A non-empty map here is still an error: that would be a real shape mismatch.
fn empty_table_as_list<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ListOrEmptyTable<T>(std::marker::PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ListOrEmptyTable<T> {
        type Value = Option<Vec<T>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a list, or an empty table")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut items = Vec::new();
            while let Some(item) = access.next_element()? {
                items.push(item);
            }
            Ok(Some(items))
        }

        fn visit_map<A: de::MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            if let Some((key, _)) = access.next_entry::<String, de::IgnoredAny>()? {
                return Err(de::Error::custom(format!(
                    "expected a list, found a table with keys (first: {key:?})"
                )));
            }
            Ok(Some(Vec::new()))
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(ListOrEmptyTable(std::marker::PhantomData))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CollectorDb {
    pub schema_version: Option<u16>,
    pub characters: BTreeMap<String, CharacterRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CharacterRecord {
    pub schema_version: Option<u16>,
    /// Unix epoch seconds.
    pub captured_at: Option<i64>,
    pub game_flavor: Option<String>,
    /// Stored raw so a flavor this reader predates can still be classified.
    pub interface_version: Option<u32>,

    pub realm: Option<String>,
    pub name: Option<String>,
    pub level: Option<u16>,
    /// Locale-independent file name, e.g. `MAGE`.
    pub class: Option<String>,
    #[serde(rename = "classID")]
    pub class_id: Option<u8>,
    pub race: Option<String>,
    #[serde(rename = "raceID")]
    pub race_id: Option<u16>,
    pub faction: Option<String>,

    pub zone: Option<String>,
    pub sub_zone: Option<String>,
    pub bind_location: Option<String>,
    pub is_resting: Option<bool>,
    pub money_copper: Option<u64>,
    pub xp: Option<u64>,
    #[serde(rename = "maxXP")]
    pub max_xp: Option<u64>,
    /// Zero when not rested; absent when not captured. The difference matters.
    #[serde(rename = "restedXP")]
    pub rested_xp: Option<u64>,

    #[serde(deserialize_with = "empty_table_as_list")]
    pub skills: Option<Vec<Skill>>,
    /// False when a collapsed header hid lines from enumeration.
    pub skills_complete: Option<bool>,
    pub talents: Option<Talents>,
    #[serde(deserialize_with = "empty_table_as_list")]
    pub quests: Option<Vec<Quest>>,
    pub quests_complete: Option<bool>,
    pub inventory: Option<Inventory>,
    pub ruleset: Option<Ruleset>,
    pub pet: Option<Pet>,
    pub recipes: Option<BTreeMap<String, ProfessionRecipes>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Skill {
    pub name: Option<String>,
    pub rank: Option<u16>,
    pub max_rank: Option<u16>,
    /// Forever supplies a stable identifier; the Classic clients do not.
    #[serde(rename = "skillID")]
    pub skill_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Talents {
    #[serde(deserialize_with = "empty_table_as_list")]
    pub trees: Option<Vec<TalentTree>>,
    pub unspent_points: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TalentTree {
    pub name: Option<String>,
    pub points_spent: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum QuestState {
    #[default]
    Active,
    Complete,
    Failed,
    /// A state this reader does not know. Kept rather than dropped so a newer
    /// addon does not make quests disappear.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Quest {
    #[serde(rename = "questID")]
    pub quest_id: Option<u32>,
    pub title: Option<String>,
    pub level: Option<u16>,
    pub header: Option<String>,
    pub state: Option<QuestState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Inventory {
    #[serde(deserialize_with = "empty_table_as_list")]
    pub bags: Option<Vec<Bag>>,
    pub total_slots: Option<u32>,
    pub free_slots: Option<u32>,
    #[serde(deserialize_with = "empty_table_as_list")]
    pub contents: Option<Vec<ItemStack>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Bag {
    pub bag_index: Option<u8>,
    #[serde(rename = "itemID")]
    pub item_id: Option<u32>,
    pub slots: Option<u32>,
    pub free_slots: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ItemStack {
    #[serde(rename = "itemID")]
    pub item_id: Option<u32>,
    pub count: Option<u32>,
}

/// Forever replaces realms with rulesets. Normal is the absence of all three,
/// not a value of its own, so `false` here is meaningful data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Ruleset {
    pub hardcore: Option<bool>,
    pub pvp: Option<bool>,
    pub rp: Option<bool>,
    pub self_found: Option<bool>,
}

impl Ruleset {
    /// True only when every flag is known and false. An unknown flag means the
    /// ruleset cannot be called normal, because it might not be.
    pub fn is_normal(&self) -> bool {
        matches!(
            (self.hardcore, self.pvp, self.rp),
            (Some(false), Some(false), Some(false))
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Pet {
    pub name: Option<String>,
    pub happiness: Option<u8>,
    pub loyalty: Option<u8>,
    pub training_points_spent: Option<u32>,
    pub training_points_total: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfessionRecipes {
    /// Recipes are a cache scraped when the profession window was open, not a
    /// live capture, so this can lag the record's own `captured_at`.
    pub captured_at: Option<i64>,
    pub rank: Option<u16>,
    pub max_rank: Option<u16>,
    #[serde(deserialize_with = "empty_table_as_list")]
    pub recipes: Option<Vec<Recipe>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Recipe {
    pub name: Option<String>,
    /// The game's own classification: optimal, medium, easy, trivial.
    pub difficulty: Option<String>,
    #[serde(rename = "spellID")]
    pub spell_id: Option<u32>,
}

/// What went wrong reading a file.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// The file is not the restricted subset.
    Syntax(lua::ParseError),
    /// The addon's global is not in the file.
    MissingGlobal(String),
    /// The shape parsed but does not match the schema.
    Shape(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax(error) => write!(f, "{error}"),
            Self::MissingGlobal(name) => write!(
                f,
                "{name} is not in this file — is it the collector's SavedVariables?"
            ),
            Self::Shape(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// A loaded file, plus anything the reader wants to say about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub db: CollectorDb,
    /// Non-fatal notes: a newer schema, a record that lost a field. These are
    /// surfaced rather than logged, because a silently degraded import is worse
    /// than a noisy one.
    pub notices: Vec<String>,
}

/// Read a collector SavedVariables file.
pub fn load(input: &str) -> Result<Loaded, LoadError> {
    let globals = lua::parse_saved_variables(input).map_err(LoadError::Syntax)?;
    let value = globals
        .get(SAVED_VARIABLE)
        .ok_or_else(|| LoadError::MissingGlobal(SAVED_VARIABLE.to_string()))?;
    load_value(value)
}

fn load_value(value: &LuaValue) -> Result<Loaded, LoadError> {
    let json = lua::to_json(value).map_err(LoadError::Shape)?;
    let db: CollectorDb = serde_json::from_value(json).map_err(|error| {
        LoadError::Shape(format!(
            "{SAVED_VARIABLE} does not match the schema: {error}"
        ))
    })?;

    let mut notices = Vec::new();
    if let Some(version) = db.schema_version {
        if version > KNOWN_SCHEMA_VERSION {
            notices.push(format!(
                "file declares schema v{version}; this reader knows v{KNOWN_SCHEMA_VERSION}, \
                 so newer fields were ignored"
            ));
        }
    } else {
        notices.push("file declares no schema version".to_string());
    }

    for (key, record) in &db.characters {
        if record.name.is_none() {
            notices.push(format!("character {key} has no name"));
        }
        if record.skills_complete == Some(false) {
            notices.push(format!(
                "{}: skills were only partly captured (a collapsed header hid some)",
                record.display_name(key)
            ));
        }
        if record.quests_complete == Some(false) {
            notices.push(format!(
                "{}: quests were only partly captured (a collapsed header hid some)",
                record.display_name(key)
            ));
        }
    }

    Ok(Loaded { db, notices })
}

impl CharacterRecord {
    /// A name to show. Falls back to the storage key, which always exists.
    pub fn display_name<'a>(&'a self, key: &'a str) -> &'a str {
        self.name.as_deref().unwrap_or(key)
    }

    /// The rested cap: 1.5 levels' worth of XP.
    ///
    /// Correct for the Classic clients. **Not established for Forever**, where
    /// the Legacy "Well Rested" perk changes both the cap and the accrual rate
    /// and nothing in the API exposes either. Callers must check
    /// [`Self::rested_cap_is_known`] before treating this as fact.
    pub fn rested_cap(&self) -> Option<u64> {
        self.max_xp.map(|max| max * 3 / 2)
    }

    /// Whether the rested model is trustworthy for this character's client.
    pub fn rested_cap_is_known(&self) -> bool {
        !matches!(self.game_flavor.as_deref(), Some("forever"))
    }

    /// How full the rested bar is, 0.0 to 1.0.
    pub fn rested_fraction(&self) -> Option<f64> {
        let cap = self.rested_cap()?;
        if cap == 0 {
            return None;
        }
        Some(self.rested_xp? as f64 / cap as f64)
    }

    pub fn free_bag_slots(&self) -> Option<u32> {
        self.inventory.as_ref()?.free_slots
    }

    pub fn quest_count(&self) -> Option<usize> {
        Some(self.quests.as_ref()?.len())
    }

    pub fn gold(&self) -> Option<u64> {
        Some(self.money_copper? / 10_000)
    }
}
