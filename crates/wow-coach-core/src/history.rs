//! Keeping what the collector would otherwise overwrite.
//!
//! The addon rewrites each character's record on every capture, so without
//! somewhere to put the previous state, history is discarded continuously.
//! Nothing here can be recovered later: a week not recorded is a week gone.
//! That is why this exists before the features that are merely useful.
//!
//! Snapshots are append-only and deduplicated by capture time, so recording
//! the same file twice is harmless and the store can be replayed from scratch.
//! The format is one JSON object per line: no database, no schema migration,
//! and the same code runs in a browser where there is no filesystem at all.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::collector::CharacterRecord;

/// A point in one character's history. Deliberately smaller than the full
/// record: this is appended forever, so it keeps what is worth trending and
/// drops what is not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub key: String,
    pub name: String,
    /// Unix epoch seconds, from the capture itself rather than from when it
    /// was recorded — so replaying an old file lands it in the right place.
    pub captured_at: i64,
    pub level: Option<u16>,
    pub xp: Option<u64>,
    pub max_xp: Option<u64>,
    pub rested_xp: Option<u64>,
    pub money_copper: Option<u64>,
    pub zone: Option<String>,
    pub quest_count: Option<usize>,
    pub free_bag_slots: Option<u32>,
    /// Profession name to rank. A map because professions come and go.
    #[serde(default)]
    pub skills: BTreeMap<String, u16>,
}

impl Snapshot {
    pub fn from_record(key: &str, record: &CharacterRecord) -> Option<Self> {
        let captured_at = record.captured_at?;
        Some(Self {
            key: key.to_string(),
            name: record.display_name(key).to_string(),
            captured_at,
            level: record.level,
            xp: record.xp,
            max_xp: record.max_xp,
            rested_xp: record.rested_xp,
            money_copper: record.money_copper,
            zone: record.zone.clone(),
            quest_count: record.quest_count(),
            free_bag_slots: record.free_bag_slots(),
            skills: record
                .skills
                .iter()
                .flatten()
                .filter_map(|skill| Some((skill.name.clone()?, skill.rank?)))
                .collect(),
        })
    }
}

/// Every snapshot we hold, in no particular order until asked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    snapshots: Vec<Snapshot>,
}

impl History {
    pub fn new(snapshots: Vec<Snapshot>) -> Self {
        Self { snapshots }
    }

    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// Add a snapshot unless the same capture is already held.
    ///
    /// Deduplicating on capture time rather than on content means running the
    /// tool repeatedly between game sessions records nothing, which is what
    /// makes automatic recording safe.
    pub fn record(&mut self, snapshot: Snapshot) -> bool {
        let duplicate = self
            .snapshots
            .iter()
            .any(|held| held.key == snapshot.key && held.captured_at == snapshot.captured_at);
        if duplicate {
            return false;
        }
        self.snapshots.push(snapshot);
        true
    }

    /// One character's snapshots, oldest first.
    pub fn for_character(&self, key: &str) -> Vec<&Snapshot> {
        let mut found: Vec<&Snapshot> = self
            .snapshots
            .iter()
            .filter(|snapshot| snapshot.key == key)
            .collect();
        found.sort_by_key(|snapshot| snapshot.captured_at);
        found
    }

    /// Every character we hold history for, with its most recent snapshot.
    pub fn latest_per_character(&self) -> BTreeMap<&str, &Snapshot> {
        let mut latest: BTreeMap<&str, &Snapshot> = BTreeMap::new();
        for snapshot in &self.snapshots {
            latest
                .entry(snapshot.key.as_str())
                .and_modify(|held| {
                    if snapshot.captured_at > held.captured_at {
                        *held = snapshot;
                    }
                })
                .or_insert(snapshot);
        }
        latest
    }
}

/// How much XP was gained between two snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XpGain {
    /// Exact: no level change, or a single level where the old maximum is known.
    Exact(u64),
    /// More than one level passed, so the intermediate level sizes are unknown
    /// and this is a floor rather than a figure.
    AtLeast(u64),
    /// Not enough captured to say.
    Unknown,
}

/// What changed between two points for one character.
#[derive(Debug, Clone, PartialEq)]
pub struct Delta {
    pub name: String,
    pub from: i64,
    pub to: i64,
    pub levels_gained: i32,
    pub xp: XpGain,
    /// Positive for earned, negative for spent.
    pub copper: Option<i64>,
    pub skills: BTreeMap<String, i32>,
    pub zone: Option<String>,
}

impl Delta {
    /// True when nothing happened worth reporting.
    pub fn is_idle(&self) -> bool {
        self.levels_gained == 0
            && matches!(self.xp, XpGain::Exact(0) | XpGain::Unknown)
            && self.copper.unwrap_or(0) == 0
            && self.skills.is_empty()
    }
}

/// Compare two snapshots of the same character.
pub fn delta(before: &Snapshot, after: &Snapshot) -> Delta {
    let levels_gained = match (before.level, after.level) {
        (Some(before), Some(after)) => i32::from(after) - i32::from(before),
        _ => 0,
    };

    // Crossing a level resets XP to near zero, so a naive subtraction reads as
    // a large loss. What was actually gained is the remainder of the old level
    // plus progress into the new one — which needs the old level's maximum.
    let xp = match (before.xp, after.xp, before.max_xp) {
        (Some(before_xp), Some(after_xp), _) if levels_gained == 0 => {
            XpGain::Exact(after_xp.saturating_sub(before_xp))
        }
        (Some(before_xp), Some(after_xp), Some(before_max)) if levels_gained == 1 => {
            XpGain::Exact(before_max.saturating_sub(before_xp) + after_xp)
        }
        (Some(before_xp), Some(after_xp), Some(before_max)) if levels_gained > 1 => {
            // The sizes of the levels in between were never captured, so this
            // is a floor. Saying "at least" beats inventing the rest.
            XpGain::AtLeast(before_max.saturating_sub(before_xp) + after_xp)
        }
        _ => XpGain::Unknown,
    };

    let copper = match (before.money_copper, after.money_copper) {
        (Some(before), Some(after)) => Some(after as i64 - before as i64),
        _ => None,
    };

    let mut skills = BTreeMap::new();
    for (name, after_rank) in &after.skills {
        let before_rank = before.skills.get(name).copied().unwrap_or(0);
        let change = i32::from(*after_rank) - i32::from(before_rank);
        if change != 0 {
            skills.insert(name.clone(), change);
        }
    }

    Delta {
        name: after.name.clone(),
        from: before.captured_at,
        to: after.captured_at,
        levels_gained,
        xp,
        copper,
        skills,
        zone: after.zone.clone(),
    }
}

/// The change across a character's whole recorded history.
pub fn total_delta(history: &History, key: &str) -> Option<Delta> {
    let snapshots = history.for_character(key);
    let (first, last) = (snapshots.first()?, snapshots.last()?);
    if first.captured_at == last.captured_at {
        return None;
    }
    Some(delta(first, last))
}

/// Characters that have been captured more than once and gained nothing.
///
/// This is the observation a snapshot cannot make and history can: not "this
/// character is behind" but "you have stopped playing this character", which
/// is only visible over time.
pub fn stalled(history: &History, since: i64) -> Vec<Delta> {
    let mut found = Vec::new();
    for key in history.latest_per_character().keys() {
        let snapshots = history.for_character(key);
        let Some(last) = snapshots.last() else {
            continue;
        };
        // Only judge a character whose window actually spans the period asked
        // about; one captured twice in an hour has not stalled, it is just new.
        let Some(first) = snapshots.iter().find(|s| s.captured_at <= since) else {
            continue;
        };
        if first.captured_at == last.captured_at {
            continue;
        }
        let change = delta(first, last);
        if change.is_idle() {
            found.push(change);
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Parse a JSON-lines store. A damaged line is skipped rather than failing the
/// whole history: losing one snapshot beats losing every snapshot.
pub fn parse_jsonl(text: &str) -> (History, Vec<String>) {
    let mut snapshots = Vec::new();
    let mut problems = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Snapshot>(line) {
            Ok(snapshot) => snapshots.push(snapshot),
            Err(error) => problems.push(format!("history line {}: {error}", index + 1)),
        }
    }
    (History::new(snapshots), problems)
}

/// Render one snapshot as a line for the store.
pub fn to_jsonl_line(snapshot: &Snapshot) -> Result<String, String> {
    serde_json::to_string(snapshot).map_err(|error| error.to_string())
}
