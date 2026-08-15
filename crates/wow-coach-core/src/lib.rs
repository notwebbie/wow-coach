use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameFlavor {
    Anniversary,
    TbcClassic,
    ClassicEra,
    Retail,
    Other(String),
}

impl GameFlavor {
    fn key(&self) -> &str {
        match self {
            Self::Anniversary => "anniversary",
            Self::TbcClassic => "tbc_classic",
            Self::ClassicEra => "classic_era",
            Self::Retail => "retail",
            Self::Other(value) => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountIdentity {
    pub installation_id: String,
    pub account_id: String,
}

impl AccountIdentity {
    pub fn new(installation_id: impl Into<String>, account_id: impl Into<String>) -> Self {
        Self {
            installation_id: normalize(installation_id.into()),
            account_id: normalize(account_id.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharacterIdentity {
    pub account: AccountIdentity,
    pub game_flavor: GameFlavor,
    pub region: String,
    pub realm: String,
    pub character_name: String,
}

impl CharacterIdentity {
    pub fn new(
        account: AccountIdentity,
        game_flavor: GameFlavor,
        region: impl Into<String>,
        realm: impl Into<String>,
        character_name: impl Into<String>,
    ) -> Self {
        Self {
            account,
            game_flavor,
            region: normalize(region.into()),
            realm: normalize(realm.into()),
            character_name: normalize(character_name.into()),
        }
    }

    /// A length-prefixed tuple key. Length prefixes prevent delimiter collisions.
    pub fn storage_key(&self) -> String {
        [
            self.account.installation_id.as_str(),
            self.account.account_id.as_str(),
            self.game_flavor.key(),
            self.region.as_str(),
            self.realm.as_str(),
            self.character_name.as_str(),
        ]
        .into_iter()
        .map(|part| format!("{}:{}", part.len(), part))
        .collect::<Vec<_>>()
        .join("|")
    }
}

fn normalize(value: String) -> String {
    value.trim().to_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfessionSnapshot {
    pub rank: u16,
    pub maximum_rank: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterSnapshot {
    pub schema_version: u16,
    pub identity: CharacterIdentity,
    pub captured_at: DateTime<Utc>,
    pub level: u16,
    pub class_id: Option<u8>,
    pub race_id: Option<u8>,
    pub faction: Option<String>,
    pub zone: Option<String>,
    pub money_copper: Option<u64>,
    pub xp: Option<u64>,
    pub max_xp: Option<u64>,
    pub rested_xp: Option<u64>,
    pub professions: BTreeMap<String, ProfessionSnapshot>,
}

impl CharacterSnapshot {
    pub fn minimal(identity: CharacterIdentity, captured_at: DateTime<Utc>, level: u16) -> Self {
        Self {
            schema_version: 1,
            identity,
            captured_at,
            level,
            class_id: None,
            race_id: None,
            faction: None,
            zone: None,
            money_copper: None,
            xp: None,
            max_xp: None,
            rested_xp: None,
            professions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub character: CharacterIdentity,
    pub snapshot: CharacterSnapshot,
}

impl HistoryEntry {
    pub fn new(
        character: CharacterIdentity,
        snapshot: CharacterSnapshot,
    ) -> Result<Self, &'static str> {
        if character != snapshot.identity {
            return Err("history character does not match snapshot identity");
        }
        Ok(Self {
            character,
            snapshot,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyReportV3 {
    pub schema_version: u16,
    pub generated_at: DateTime<Utc>,
    pub app_version: String,
    pub installation_flavor: String,
    #[serde(default)]
    pub accounts: Vec<LegacyAccountV3>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyAccountV3 {
    pub account: String,
    pub source_path: String,
    #[serde(default)]
    pub characters: Vec<LegacyCharacterV3>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyCharacterV3 {
    pub name: String,
    pub realm: Option<String>,
    pub faction: Option<String>,
    pub race: Option<String>,
    pub character_class: Option<String>,
    pub level: Option<u16>,
    pub money_copper: Option<u64>,
    pub zone: Option<String>,
}
