//! What to do with a quest log, and what to stop carrying in it.
//!
//! A quest log is not a list, it is a route. Fifteen quests spread over six
//! zones is six trips; the same fifteen with ten in one zone is one trip and a
//! bit. The client already groups the log under zone headers and the collector
//! keeps them, so the grouping is the game's own rather than a guess.
//!
//! **A line earns its place only if it names something to do.** The prototype
//! reported "N orange/red active quests" on every character, which presents an
//! above-level quest as a fault when it is usually just a quest you have not
//! got to yet. Here, a quest being above your level is only worth mentioning
//! when the log is too full to take anything new — at which point it stops
//! being trivia and becomes the answer to "what do I drop".
//!
//! # The level bands are ours, not the client's
//!
//! The game colours quests through `GetQuestDifficultyColor`, which the
//! collector does not capture — it stores the quest's level and leaves
//! judgement to the rules, which is the whole point of keeping facts and
//! judgements apart. So the bands below are this project's, they are named
//! constants with reasoning, and they are deliberately wide: the cost of
//! calling a doable quest "too high" is that somebody drops something they
//! could have finished.

use std::collections::BTreeMap;

use crate::collector::{CharacterRecord, Quest, QuestState};

/// More than this above the player is where a quest starts needing either a
/// group or more levels. Four is the width of the game's own orange band, so
/// anything past it is the red one.
const TOO_HIGH_BY: u16 = 4;

/// At least this far below the player before a quest is worth calling low
/// value. Wider than the game's green band on purpose — a quest five levels
/// down still pays, and telling somebody to bin it is worse than saying
/// nothing.
const TRIVIAL_BY: u16 = 8;

/// Within this many slots of the cap, the log stops being able to take work.
/// Two rather than zero: a full log is already a problem, and the point is to
/// say so before the player is standing in front of a quest giver.
const TIGHT_WITHIN: usize = 2;

/// How a quest sits against the player's level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Far enough above that it likely needs a group or more levels.
    TooHigh { by: u16 },
    /// Within reach.
    Ready,
    /// Far enough below that it pays little.
    Trivial { by: u16 },
    /// No level was captured, so nothing can be said.
    Unknown,
}

impl Standing {
    fn of(quest_level: Option<u16>, player_level: Option<u16>) -> Self {
        let (Some(quest), Some(player)) = (quest_level, player_level) else {
            return Self::Unknown;
        };
        if quest > player + TOO_HIGH_BY {
            return Self::TooHigh { by: quest - player };
        }
        if player >= quest && player - quest >= TRIVIAL_BY {
            return Self::Trivial { by: player - quest };
        }
        Self::Ready
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestRef {
    pub title: String,
    pub level: Option<u16>,
    pub zone: Option<String>,
    pub standing: Standing,
}

/// Every quest sharing one of the client's own zone headers.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneCluster {
    pub zone: String,
    /// Done, waiting to be handed in.
    pub complete: Vec<QuestRef>,
    /// Active and at a level worth attempting.
    pub ready: Vec<QuestRef>,
    pub too_high: Vec<QuestRef>,
    pub trivial: Vec<QuestRef>,
}

impl ZoneCluster {
    /// How much of this zone a visit could actually clear. Quests that are too
    /// high or barely pay do not count towards a reason to go.
    pub fn worth_a_trip(&self) -> usize {
        self.complete.len() + self.ready.len()
    }

    pub fn total(&self) -> usize {
        self.complete.len() + self.ready.len() + self.too_high.len() + self.trivial.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogPressure {
    pub used: usize,
    pub capacity: usize,
}

impl LogPressure {
    /// True when the log can take almost nothing new.
    pub fn is_tight(&self) -> bool {
        self.used + TIGHT_WITHIN >= self.capacity
    }

    pub fn free(&self) -> usize {
        self.capacity.saturating_sub(self.used)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestPlan {
    pub character: String,
    pub level: Option<u16>,
    /// Ready to hand in, wherever they are. Always worth doing first: they
    /// cost nothing but the walk and they free a slot each.
    pub turn_ins: Vec<QuestRef>,
    /// Zones, most worth visiting first.
    pub clusters: Vec<ZoneCluster>,
    pub log: LogPressure,
    /// What to let go of — **only** populated when the log is tight. A quest
    /// you are not going to do is harmless until it costs you a slot.
    pub drop_candidates: Vec<QuestRef>,
    pub caveats: Vec<String>,
}

impl QuestPlan {
    /// The zone that clears the most, if any zone clears anything.
    pub fn best_zone(&self) -> Option<&ZoneCluster> {
        self.clusters
            .first()
            .filter(|cluster| cluster.worth_a_trip() > 0)
    }
}

fn quest_ref(quest: &Quest, player_level: Option<u16>) -> Option<QuestRef> {
    let title = quest.title.clone()?;
    Some(QuestRef {
        title,
        level: quest.level,
        zone: quest.header.clone(),
        standing: Standing::of(quest.level, player_level),
    })
}

/// Plan one character's quest log.
pub fn plan(record: &CharacterRecord, key: &str) -> QuestPlan {
    let level = record.level;
    let quests = record.quests.as_deref().unwrap_or(&[]);
    let capacity = crate::coaching::quest_log_capacity(record.game_flavor.as_deref());

    let mut turn_ins = Vec::new();
    let mut by_zone: BTreeMap<String, ZoneCluster> = BTreeMap::new();

    for quest in quests {
        let Some(entry) = quest_ref(quest, level) else {
            continue;
        };
        // The client groups the log under headers that are usually zones but
        // are sometimes a class or a dungeon. Whatever it said is used as-is:
        // reclassifying it would be inventing structure the game did not give.
        let zone = entry
            .zone
            .clone()
            .unwrap_or_else(|| "Elsewhere".to_string());
        let cluster = by_zone.entry(zone.clone()).or_insert_with(|| ZoneCluster {
            zone,
            complete: Vec::new(),
            ready: Vec::new(),
            too_high: Vec::new(),
            trivial: Vec::new(),
        });

        if quest.state == Some(QuestState::Complete) {
            turn_ins.push(entry.clone());
            cluster.complete.push(entry);
            continue;
        }
        match entry.standing {
            Standing::TooHigh { .. } => cluster.too_high.push(entry),
            Standing::Trivial { .. } => cluster.trivial.push(entry),
            Standing::Ready | Standing::Unknown => cluster.ready.push(entry),
        }
    }

    let mut clusters: Vec<ZoneCluster> = by_zone.into_values().collect();
    clusters.sort_by(|a, b| {
        b.worth_a_trip()
            .cmp(&a.worth_a_trip())
            .then_with(|| b.total().cmp(&a.total()))
            .then_with(|| a.zone.cmp(&b.zone))
    });

    let log = LogPressure {
        used: quests.len(),
        capacity,
    };

    // Only under pressure. A quest you are never going to do costs nothing
    // until it costs you a slot, and nagging about it before then is how a
    // useful tool becomes a noisy one.
    let mut drop_candidates = Vec::new();
    if log.is_tight() {
        for cluster in &clusters {
            drop_candidates.extend(cluster.too_high.iter().cloned());
        }
        for cluster in &clusters {
            drop_candidates.extend(cluster.trivial.iter().cloned());
        }
        // Furthest out of reach first, then the most trivial.
        drop_candidates.sort_by_key(|entry| match entry.standing {
            Standing::TooHigh { by } => (0i32, -(by as i32)),
            Standing::Trivial { by } => (1i32, -(by as i32)),
            _ => (2i32, 0),
        });
    }

    QuestPlan {
        character: record.display_name(key).to_string(),
        level,
        caveats: caveats(record, &log, &turn_ins, &drop_candidates),
        turn_ins,
        clusters,
        log,
        drop_candidates,
    }
}

fn caveats(
    record: &CharacterRecord,
    log: &LogPressure,
    turn_ins: &[QuestRef],
    drops: &[QuestRef],
) -> Vec<String> {
    let mut caveats = Vec::new();

    if record.quests.is_none() {
        caveats.push("The quest log was not captured for this character.".to_string());
        return caveats;
    }

    if record.quests_complete == Some(false) {
        caveats.push(
            "A collapsed header hid part of the quest log when this was captured, so some quests are missing here. Expand every header in game and log out again."
                .to_string(),
        );
    }

    if log.is_tight() {
        let freed = turn_ins.len();
        if freed > 0 {
            caveats.push(format!(
                "The log is {}/{}. Handing in the {freed} finished quest(s) frees that many slots on its own.",
                log.used, log.capacity
            ));
        } else {
            caveats.push(format!(
                "The log is {}/{} and nothing is ready to hand in, so taking anything new means dropping something.",
                log.used, log.capacity
            ));
        }
    }

    if !drops.is_empty() {
        caveats.push(
            "Quests are only listed as droppable because the log is full. Out of reach today is not out of reach forever, and a dropped quest has to be picked up again."
                .to_string(),
        );
    }

    caveats
}
