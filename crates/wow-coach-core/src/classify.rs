//! Proposing roles, so nobody has to tag eighteen characters blind.
//!
//! Marking a roster by hand is a chore people do not do, and an untagged
//! roster makes every rotation suggestion worse — a bank alt sitting at the
//! rested cap will be recommended forever. The tool already holds the evidence
//! to guess: level, quest log, whether it is resting, and now whether it has
//! actually gained anything over time.
//!
//! **Inference proposes, the player disposes.** Nothing here writes a role.
//! Every suggestion carries the evidence that produced it and a confidence,
//! because a guess presented as a fact is worse than no guess at all — and a
//! character that is quiet because it is a bank alt looks identical to one
//! that is quiet because its owner was on holiday.

use crate::collector::CharacterRecord;
use crate::history::{self, History};
use crate::roster::{Role, RosterConfig, RosterEntry};

/// A character below this level with nothing in its quest log is far more
/// likely to be storage than a character somebody is levelling.
const STORAGE_LEVEL: u16 = 5;

/// How long a character has to be observably idle before silence means
/// anything. Shorter than this and it is a quiet week, not a decision.
pub const IDLE_SECONDS: i64 = 14 * 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Backed by observed history: captured across a period and gained nothing.
    Observed,
    /// Consistent with the current snapshot, but nothing confirms it.
    Possible,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoleSuggestion {
    pub key: String,
    pub name: String,
    pub current: Role,
    pub suggested: Role,
    pub confidence: Confidence,
    /// What led here, in the character's own numbers.
    pub evidence: Vec<String>,
}

fn idle_for(history: &History, key: &str, now: i64) -> Option<i64> {
    let snapshots = history.for_character(key);
    let first = snapshots.first()?;
    let last = snapshots.last()?;
    if first.captured_at == last.captured_at {
        return None;
    }
    let span = last.captured_at - first.captured_at;
    if span < IDLE_SECONDS {
        return None;
    }
    // Only silence across a real span counts. A character captured twice in an
    // hour has not been abandoned, it is simply new.
    let _ = now;
    history::delta(first, last).is_idle().then_some(span)
}

fn looks_like_storage(record: &CharacterRecord) -> Option<Vec<String>> {
    let level = record.level?;
    if level > STORAGE_LEVEL {
        return None;
    }
    let quests = record.quest_count().unwrap_or(0);
    if quests > 0 {
        return None;
    }
    Some(vec![format!(
        "level {level} with an empty quest log, which is what a storage character looks like"
    )])
}

/// Suggest a role for one character, or nothing when it looks active.
pub fn suggest(entry: &RosterEntry<'_>, history: &History, now: i64) -> Option<RoleSuggestion> {
    let record = entry.record;
    let mut evidence = Vec::new();

    let idle = idle_for(history, entry.key, now);
    if let Some(span) = idle {
        evidence.push(format!(
            "captured across {} days and gained no XP, levels, gold or skill in that time",
            span / 86_400
        ));
    }

    let storage = looks_like_storage(record);
    if let Some(reasons) = &storage {
        evidence.extend(reasons.iter().cloned());
    }
    if record.is_resting == Some(false) {
        evidence.push("not parked in an inn or city, so it is banking no rest".to_string());
    }

    let (suggested, confidence) = match (idle.is_some(), storage.is_some()) {
        // Quiet and looks like storage: a bank alt.
        (true, true) => (Role::Bank, Confidence::Observed),
        // Quiet but a real character: set down rather than stored.
        (true, false) => (Role::Parked, Confidence::Observed),
        // Looks like storage, but nothing confirms it is not being levelled
        // right now. Worth asking about; not worth assuming.
        (false, true) => (Role::Bank, Confidence::Possible),
        (false, false) => return None,
    };

    if suggested == entry.role {
        return None;
    }

    Some(RoleSuggestion {
        key: entry.key.to_string(),
        name: entry.name().to_string(),
        current: entry.role,
        suggested,
        confidence,
        evidence,
    })
}

/// Suggest roles for every character that has not been classified yet.
///
/// A character the player has already decided about is left alone, whatever
/// the evidence says. Re-proposing a decision somebody already made is how a
/// helpful tool becomes a nagging one.
pub fn suggest_all(
    entries: &[RosterEntry<'_>],
    config: &RosterConfig,
    history: &History,
    now: i64,
) -> Vec<RoleSuggestion> {
    let mut suggestions: Vec<RoleSuggestion> = entries
        .iter()
        .filter(|entry| !config.roles.contains_key(entry.key))
        .filter_map(|entry| suggest(entry, history, now))
        .collect();
    // Observed first: those are the ones worth reading carefully.
    suggestions.sort_by(|a, b| {
        a.confidence
            .cmp_order()
            .cmp(&b.confidence.cmp_order())
            .then_with(|| a.name.cmp(&b.name))
    });
    suggestions
}

impl Confidence {
    fn cmp_order(&self) -> u8 {
        match self {
            Self::Observed => 0,
            Self::Possible => 1,
        }
    }

    pub fn describe(&self) -> &'static str {
        match self {
            Self::Observed => "observed over time",
            Self::Possible => "possible, not confirmed",
        }
    }
}
