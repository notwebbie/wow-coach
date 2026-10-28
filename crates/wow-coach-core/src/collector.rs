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
/// Read a recipe difficulty whether it arrives as a word or as an enum value.
///
/// 0 Optimal, 1 Medium, 2 Easy, 3 Trivial — the modern client's
/// `Enum.TradeskillRelativeDifficulty`, verified against its API page. A
/// number outside that range becomes `None` rather than a guess, which the
/// reader already treats as "unrated, might still raise the skill".
fn difficulty_word<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum WordOrEnum {
        Word(String),
        Code(i64),
    }

    Ok(match Option::<WordOrEnum>::deserialize(deserializer)? {
        Some(WordOrEnum::Word(word)) => Some(word),
        Some(WordOrEnum::Code(0)) => Some("optimal".to_string()),
        Some(WordOrEnum::Code(1)) => Some("medium".to_string()),
        Some(WordOrEnum::Code(2)) => Some("easy".to_string()),
        Some(WordOrEnum::Code(3)) => Some("trivial".to_string()),
        Some(WordOrEnum::Code(_)) | None => None,
    })
}

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

    /// Every spell the character knows, by id. Absent means not captured —
    /// never "knows nothing".
    #[serde(deserialize_with = "empty_table_as_list")]
    pub spells: Option<Vec<KnownSpell>>,
    #[serde(deserialize_with = "empty_table_as_list")]
    pub skills: Option<Vec<Skill>>,
    /// False when a collapsed header hid lines from enumeration.
    pub skills_complete: Option<bool>,
    /// The skills the game itself files as professions.
    ///
    /// Absent means the client has no `GetProfessions`, or the capture
    /// predates this field. **An empty list means nothing on its own** — see
    /// [`Self::profession_list_is_usable`], which is the only safe way to read
    /// it.
    #[serde(deserialize_with = "empty_table_as_list")]
    pub professions: Option<Vec<ProfessionSlot>>,
    /// How many values `GetProfessions` returned, nils included.
    ///
    /// The fact that separates "asked, and this character has none" from "this
    /// client's `GetProfessions` is a stub". A working client returns its full
    /// slot list even when every slot is empty; a stub returns nothing.
    /// Absent on captures predating this field.
    pub professions_returned: Option<u8>,
    pub talents: Option<Talents>,
    /// WoW Forever's account-wide perk trees. Absent on every other client.
    #[serde(deserialize_with = "empty_table_as_list")]
    pub legacy: Option<Vec<LegacyTree>>,
    #[serde(deserialize_with = "empty_table_as_list")]
    pub quests: Option<Vec<Quest>>,
    pub quests_complete: Option<bool>,
    pub inventory: Option<Inventory>,
    /// What the character is wearing. Absent means the capture never ran, and
    /// is not the same as a character wearing nothing — [`Equipment::slots_read`]
    /// is what tells those apart.
    pub equipment: Option<Equipment>,
    /// The bank, as of the last time its window was open. Absent means it has
    /// never been seen, which is not the same as empty and must never be shown
    /// as such.
    pub bank: Option<Bank>,
    pub ruleset: Option<Ruleset>,
    pub pet: Option<Pet>,
    pub recipes: Option<BTreeMap<String, ProfessionRecipes>>,
}

/// One of WoW Forever's account-wide Legacy perk trees.
///
/// Captured for one question: has this character bought anything that changes
/// the rested model? The "Well Rested" perk alters both the fill rate and the
/// cap, and both were measured on a character with none — so a player who has
/// it is otherwise ranked against a baseline that does not apply to them.
///
/// Answering that needs no names. If every node reads zero, nothing is bought.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LegacyTree {
    /// `professions`, `adventure` or `resourcefulness`.
    pub tree: Option<String>,
    #[serde(rename = "treeID")]
    pub tree_id: Option<u32>,
    /// Absent when the client would not hand one over, which is not the same
    /// as a tree with nothing bought in it.
    #[serde(rename = "configID")]
    pub config_id: Option<i64>,
    pub available: Option<u32>,
    pub cap: Option<u32>,
    pub spent: Option<u32>,
    /// Absent when the tree was unreadable *or* the node API failed. Empty is
    /// a real answer: a tree with no nodes in it.
    #[serde(deserialize_with = "empty_table_as_list")]
    pub nodes: Option<Vec<LegacyNode>>,
}

impl LegacyTree {
    /// Points committed in this tree, counting nodes rather than trusting the
    /// currency total — they should agree, and the nodes are the primary fact.
    pub fn ranks_purchased(&self) -> Option<u32> {
        let nodes = self.nodes.as_ref()?;
        Some(nodes.iter().filter_map(|node| node.rank).sum())
    }
}

/// The Legacy perk that changes rested accrual, by the spell behind it.
///
/// Identified from a live capture on WoW Forever, 19 October 2026: node
/// `110301` in the Adventure tree (1188) resolves through the client's own
/// spell name to "Well Rested", 5 ranks. The spell id is matched rather than
/// the node id because a node is a position in a tree and a tree can be
/// rearranged, where the spell is the perk itself. Both are recorded in
/// `docs/SOURCES.md`.
///
/// Matched on the id and never on the name: "Well Rested" is English, and the
/// same capture carries six nodes the client itself names "Unknown".
pub const WELL_RESTED_SPELL: u32 = 1_225_478;

/// The node `WELL_RESTED_SPELL` sat on in that capture. Kept as a fallback for
/// a client that reports no spell id, and as the thing to check first if the
/// spell id ever stops matching.
pub const WELL_RESTED_NODE: u32 = 110_301;

/// One node in a Legacy tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LegacyNode {
    #[serde(rename = "nodeID")]
    pub node_id: Option<u32>,
    /// The client's own words, for a reader. Never matched on.
    pub name: Option<String>,
    #[serde(rename = "spellID")]
    pub spell_id: Option<u32>,
    pub rank: Option<u32>,
    pub max_rank: Option<u32>,
}

/// One profession, as the game's own profession list reports it.
///
/// This is the answer to a question the skill list cannot settle: whether a
/// skill is a trade skill or a language. The skill window mixes professions,
/// languages, weapon proficiencies and armour types together, and the only way
/// to tell them apart there is to read a name in the player's own locale.
/// `GetProfessions` returns professions and nothing else, which makes this the
/// signal the coaching rules can act on.
///
/// `slot` is the position the client returned it in and carries no meaning
/// here. The documented order is prof1, prof2, archaeology, fishing, cooking,
/// first aid; a live Forever capture put First Aid in slot 3. Whichever is
/// right, nothing reads the position — identity comes from `skill_line` and
/// `name`, which the client supplies per index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfessionSlot {
    /// Position in the client's own return list. A fact, not a meaning.
    pub slot: Option<u8>,
    /// The index the client wants back when asked about this profession.
    /// Documented as unstable across learning and unlearning, so it is stored
    /// for completeness and never used as a key.
    pub index: Option<u8>,
    pub name: Option<String>,
    pub rank: Option<u16>,
    pub max_rank: Option<u16>,
    /// The locale-independent identity, where the client gives one.
    pub skill_line: Option<u32>,
}

impl CharacterRecord {
    /// Whether the profession list may be treated as the client's answer.
    ///
    /// A non-empty list always may. An **empty** one may only when the client
    /// said how many slots it looked at — because an empty list and a stub
    /// that returns nothing are the same bytes otherwise, and this project
    /// learned the difference the expensive way. The Anniversary client
    /// (interface 20506) has a `GetProfessions` that exists and returns
    /// nothing; two characters carrying Tailoring, Enchanting, Mining and
    /// Skinning between them both captured as empty, and an earlier version of
    /// this file called that "the character has no professions".
    ///
    /// Captures predating `professionsReturned` have no arity, so an empty
    /// list from one is unusable — correctly, since those are exactly the
    /// captures the mistake was made against.
    /// Ranks bought in the Legacy perk that changes rested accrual.
    ///
    /// `None` when the trees were not captured or not readable — every client
    /// but Forever, and any capture predating the field.
    pub fn well_rested_rank(&self) -> Option<u32> {
        let trees = self.legacy.as_ref()?;
        let mut found = None;
        for node in trees
            .iter()
            .filter_map(|tree| tree.nodes.as_ref())
            .flatten()
        {
            let matches = node.spell_id == Some(WELL_RESTED_SPELL)
                || (node.spell_id.is_none() && node.node_id == Some(WELL_RESTED_NODE));
            if matches {
                found = Some(node.rank.unwrap_or(0));
            }
        }
        found
    }

    /// Whether the no-perk rested baseline applies to this character.
    ///
    /// **Only Well Rested counts.** The first version of this summed ranks
    /// across all three Legacy trees, and the first live capture showed why
    /// that is wrong: the character had one point in *Working Overtime*, a
    /// professions perk with nothing to do with rested XP, and would have been
    /// warned that his rested figures were approximate when they were exact.
    ///
    /// `None` when the trees were not captured, or when they were captured and
    /// the perk was not among them — an absent perk in a readable tree is a
    /// changed tree, not a zero.
    pub fn rested_baseline_applies(&self) -> Option<bool> {
        Some(self.well_rested_rank()? == 0)
    }

    pub fn profession_list_is_usable(&self) -> bool {
        match &self.professions {
            None => false,
            Some(list) if !list.is_empty() => true,
            Some(_) => self.professions_returned.is_some_and(|n| n > 0),
        }
    }
}

impl ProfessionSlot {
    /// Whether this slot describes the same profession as a captured skill
    /// line.
    ///
    /// Id first: both numbers come from the same client in the same session,
    /// so when both are present the match is exact and locale-free. Name is
    /// the fallback for the Classic clients, which give skills no id — and it
    /// is safe in a way a hard-coded name list would not be, because both
    /// sides are the client's own words for the same thing.
    pub fn is_same_as(&self, skill: &Skill) -> bool {
        match (self.skill_line, skill.skill_id) {
            (Some(mine), Some(theirs)) if mine > 0 && theirs > 0 => mine == theirs,
            _ => match (self.name.as_deref(), skill.name.as_deref()) {
                (Some(mine), Some(theirs)) => mine == theirs,
                _ => false,
            },
        }
    }
}

/// One spell the character knows.
///
/// The id is the fact. The name is whatever the client had cached, in the
/// client's own language, and must never be matched on — which is the point of
/// storing ids at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct KnownSpell {
    #[serde(rename = "spellID")]
    pub spell_id: Option<u32>,
    pub name: Option<String>,
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
    /// The skill window header this sat under — the game's own grouping.
    /// Locale-dependent: group by it, never match on it.
    pub header: Option<String>,
    /// The same grouping as a number, where the client offers one.
    #[serde(rename = "categoryID")]
    pub category_id: Option<u32>,
}

impl Skill {
    /// True when the skill is at its cap and cannot advance without training.
    pub fn is_capped(&self) -> bool {
        matches!((self.rank, self.max_rank), (Some(rank), Some(max)) if max > 0 && rank >= max)
    }

    /// How close to the cap, as a fraction. `None` when either end is missing.
    pub fn fraction(&self) -> Option<f64> {
        let (rank, max) = (self.rank?, self.max_rank?);
        if max == 0 {
            return None;
        }
        Some(f64::from(rank) / f64::from(max))
    }
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
    /// Every talent in the tree, learned or not. Absent on records written
    /// before per-talent capture, and on Forever when the node API failed —
    /// "not captured", never "no talents".
    #[serde(deserialize_with = "empty_table_as_list")]
    pub talents: Option<Vec<Talent>>,
}

/// One talent as the talent frame lays it out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Talent {
    pub name: Option<String>,
    /// 1-based row. Row `n` opens once `5 * (n - 1)` points are in the tree.
    pub tier: Option<u8>,
    /// 1-based column, for display and for matching `requires`.
    pub column: Option<u8>,
    /// Committed points only.
    pub rank: Option<u8>,
    pub max_rank: Option<u8>,
    /// Forever only; a locale-independent identity.
    #[serde(rename = "spellID")]
    pub spell_id: Option<u32>,
    /// The talent an arrow points from, which must be maxed first.
    pub requires: Option<TalentPosition>,
    /// Forever only: the node's raw coordinates in the talent frame, kept so
    /// the grid can be recomputed if the collector's grouping is ever wrong.
    #[serde(rename = "posX")]
    pub pos_x: Option<f64>,
    #[serde(rename = "posY")]
    pub pos_y: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TalentPosition {
    pub tier: u8,
    pub column: u8,
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

/// What the character has equipped.
///
/// Only slots holding something appear in `slots`, so an empty list is a
/// character wearing nothing — but only when `slots_read` says the question was
/// asked. Both absent means no capture has run, and the two must never be
/// shown the same way: "you have no wand" and "nobody has looked" send a
/// player in opposite directions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Equipment {
    #[serde(deserialize_with = "empty_table_as_list")]
    pub slots: Option<Vec<EquippedItem>>,
    /// How many slots the collector asked about. The arity field, exactly like
    /// `professionsReturned`: without it an empty list is unreadable.
    pub slots_read: Option<u8>,
}

impl Equipment {
    /// Whether an empty `slots` list can be believed as "wearing nothing".
    pub fn is_usable(&self) -> bool {
        match (&self.slots, self.slots_read) {
            (Some(slots), _) if !slots.is_empty() => true,
            (_, Some(read)) => read > 0,
            _ => false,
        }
    }

    pub fn worn(&self) -> &[EquippedItem] {
        self.slots.as_deref().unwrap_or(&[])
    }

    /// The item in a slot, if anything is in it.
    pub fn in_slot(&self, slot: u8) -> Option<&EquippedItem> {
        self.worn().iter().find(|item| item.slot == Some(slot))
    }
}

/// One worn item.
///
/// `item_id` is the fact. Everything else comes from the client's item cache
/// and is absent when that cache has not loaded the item, which happens often
/// enough that no rule may assume it is present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EquippedItem {
    pub slot: Option<u8>,
    #[serde(rename = "itemID")]
    pub item_id: Option<u32>,
    pub name: Option<String>,
    pub quality: Option<u8>,
    pub item_level: Option<u16>,
    /// The level needed to equip it, which is what tells a hand-me-down from a
    /// current upgrade.
    pub min_level: Option<u16>,
    pub item_type: Option<String>,
    /// `Wands`, `Bows`, `Staves`, `Cloth` — the client's own word for what the
    /// item is. Matching on this rather than on a slot number is what keeps
    /// the rules working if Forever moves a wand somewhere else.
    pub item_sub_type: Option<String>,
    /// `INVTYPE_2HWEAPON`, `INVTYPE_RANGEDRIGHT` — the client's own word for
    /// the kind of slot it occupies.
    pub equip_loc: Option<String>,
}

impl EquippedItem {
    /// Case-insensitive, because the client's capitalisation is not a contract.
    pub fn is_sub_type(&self, want: &str) -> bool {
        self.item_sub_type
            .as_deref()
            .is_some_and(|got| got.eq_ignore_ascii_case(want))
    }

    pub fn is_equip_loc(&self, want: &str) -> bool {
        self.equip_loc
            .as_deref()
            .is_some_and(|got| got.eq_ignore_ascii_case(want))
    }

    /// True when the client told us what this item is. False means the cache
    /// had not loaded it, so no rule may conclude anything from its type.
    pub fn type_is_known(&self) -> bool {
        self.item_sub_type.is_some() || self.equip_loc.is_some()
    }
}

/// The bank, and when it was last looked at.
///
/// The bank differs from the bags in one way that matters to every reader: it
/// can only be read standing at a banker, so what is stored here is always a
/// memory rather than a current reading. The timestamp is therefore not a
/// nicety. It is the difference between "you have 40 Linen" and "you had 40
/// Linen three weeks ago", and a reader that drops it is making a claim the
/// data does not support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Bank {
    #[serde(deserialize_with = "empty_table_as_list")]
    pub bags: Option<Vec<Bag>>,
    pub total_slots: Option<u32>,
    pub free_slots: Option<u32>,
    #[serde(deserialize_with = "empty_table_as_list")]
    pub contents: Option<Vec<ItemStack>>,
    /// Unix seconds at which the bank window was last read.
    pub captured_at: Option<i64>,
}

impl Bank {
    /// How long ago this snapshot was taken, in days.
    pub fn age_days(&self, now: i64) -> Option<i64> {
        let at = self.captured_at?;
        Some(((now - at).max(0)) / 86_400)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Bag {
    /// Signed, because the bank's containers are negative: `-1` is the main
    /// bank window and `-3` the reagent bank. A `u8` here parsed every bag
    /// fine and rejected the whole file the moment a bank was captured.
    pub bag_index: Option<i8>,
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
    /// Absent when the client had not loaded the item yet. The collector does
    /// not wait for it, so this is a convenience and the id is the fact.
    pub name: Option<String>,
}

/// Forever replaces realms with rulesets. Normal is the absence of all three,
/// not a value of its own, so `false` here is meaningful data.
///
/// **Never observed true.** Every capture taken during the beta — four
/// characters, many logins — reports all four flags false, because the beta ran
/// Normal only: PvP realms were not available, Roleplay arrives at launch and
/// Hardcore later still. So [`Self::is_normal`] returning the right answer for
/// a non-Normal character is an assumption no data has tested, and the beta
/// window closed without being able to test it. Recorded here rather than left
/// on a list of things to try, because it was not doable rather than not done.
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
    ///
    /// Accepts a number as well as a word, and this tolerance is the point.
    /// The modern client reports `Enum.TradeskillRelativeDifficulty` — a plain
    /// integer — where Classic reports a string, and a collector that sent the
    /// raw enum made this one field reject the entire capture: every
    /// character, every bag, every quest, over one recipe's difficulty.
    ///
    /// The addon now normalises it, so this should not arrive as a number
    /// again. It stays lenient anyway, because the version policy in
    /// `docs/COLLECTOR-SCHEMA.md` promises a player who updates the addon
    /// first still sees their data, and that promise is worth nothing if a
    /// field's *type* can do what a version bump is not allowed to do.
    #[serde(deserialize_with = "difficulty_word")]
    pub difficulty: Option<String>,
    #[serde(rename = "spellID")]
    pub spell_id: Option<u32>,
    /// What it consumes. Absent means the reagents were not captured, never
    /// that the recipe is free.
    #[serde(deserialize_with = "empty_table_as_list")]
    pub reagents: Option<Vec<Reagent>>,
    /// What it produces. Absent for a recipe with no item output — an enchant
    /// applied straight to gear makes nothing to sell.
    #[serde(rename = "makesItemID")]
    pub makes_item_id: Option<u32>,
    pub makes_min: Option<u32>,
    pub makes_max: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Reagent {
    #[serde(rename = "itemID")]
    pub item_id: Option<u32>,
    /// The client's name for it, when the capture had one. Never a key.
    pub name: Option<String>,
    /// How many the recipe requires — never how many anyone is carrying.
    pub count: Option<u32>,
    /// How many interchangeable items this slot accepts, when more than one.
    /// Present only on clients that model a slot as a set of choices.
    pub choices: Option<u32>,
}

impl Recipe {
    /// The middle of the yield range, for valuing a craft.
    ///
    /// A recipe that makes one to two of a thing is worth its average, not its
    /// best case. Absent when nothing said how many it makes — which is not
    /// the same as one.
    pub fn makes_typical(&self) -> Option<u32> {
        match (self.makes_min, self.makes_max) {
            (Some(low), Some(high)) if high >= low => Some((low + high) / 2),
            (Some(only), None) | (None, Some(only)) => Some(only),
            _ => None,
        }
    }
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
        let model = crate::rested::model_for(self.game_flavor.as_deref());
        model.cap_xp(self.max_xp?)
    }

    /// Whether the rested model is trustworthy for this character's client.
    ///
    /// This asks about the **cap**, not the rate. The cap is what the ranking
    /// needs, because "how much of it is left" is what decides whether parked
    /// time is being wasted — so a client whose fill rate has been measured
    /// but whose cap has not is still unknown here, and deliberately so.
    pub fn rested_cap_is_known(&self) -> bool {
        crate::rested::model_for(self.game_flavor.as_deref())
            .cap_basis
            .is_known()
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
