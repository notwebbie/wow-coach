//! Deciding which character to play next, and saying why.
//!
//! The rule that matters: **rank by what is being lost, not by whose turn it
//! is.** Rested XP stops accruing at the cap, so a character sitting at the cap
//! is wasting every hour it stays parked, while one still filling is doing
//! exactly its job by being left alone. The prototype computed rested XP and
//! then chose alphabetically, quoting the rested figure as if it had been the
//! reason — this module exists so that cannot happen again.
//!
//! Every suggestion carries its arithmetic. A recommendation that cannot say
//! which numbers produced it is indistinguishable from a guess, and the reader
//! has no way to tell when it is wrong.
//!
//! Where the rested model is not established — Forever, where the Legacy
//! "Well Rested" perk changes both the cap and the fill rate and neither is
//! readable — this says so rather than ranking on a number it invented.

use crate::collector::CharacterRecord;
use crate::roster::{Role, RosterEntry};

/// Above this fraction of the cap, rest has effectively stopped accruing.
/// Not 1.0: the client reports rested in whole XP, so the last fraction of a
/// percent is rounding rather than meaningful headroom.
const AT_CAP: f64 = 0.99;

/// Close enough that it will cap shortly and is worth planning around.
const NEAR_CAP: f64 = 0.85;

/// How full a quest log has to be before it blocks picking anything up.
/// The cap differs by client, so this is derived rather than assumed.
fn quest_log_capacity(flavor: Option<&str>) -> usize {
    match flavor {
        // Forever raised it well above the vanilla 20.
        Some("forever") => 40,
        // Classic and TBC.
        _ => 25,
    }
}

/// What the rested bar is doing, and whether we can say.
#[derive(Debug, Clone, PartialEq)]
pub enum Rested {
    /// Rest has stopped accruing. Every parked hour from here is wasted.
    AtCap { rested: u64, cap: u64 },
    /// Will cap soon.
    NearCap { rested: u64, cap: u64 },
    /// Still filling, and therefore doing its job while parked.
    Filling { rested: u64, cap: u64 },
    /// Not in an inn or city, so it is not accruing at all.
    NotResting { rested: u64 },
    /// The model is not established for this client, or nothing was captured.
    Unknown { why: String },
}

impl Rested {
    fn fraction(rested: u64, cap: u64) -> f64 {
        if cap == 0 {
            return 0.0;
        }
        rested as f64 / cap as f64
    }

    /// Order for ranking. Higher sorts first.
    fn weight(&self) -> u8 {
        match self {
            Self::AtCap { .. } => 3,
            Self::NearCap { .. } => 2,
            Self::Filling { .. } => 1,
            Self::NotResting { .. } | Self::Unknown { .. } => 0,
        }
    }
}

/// Read the rested state of one character.
pub fn rested_state(record: &CharacterRecord) -> Rested {
    if !record.rested_cap_is_known() {
        return Rested::Unknown {
            why: "the rested cap and fill rate are not established on this client — \
                  the Legacy \"Well Rested\" perk changes both and neither is readable"
                .to_string(),
        };
    }
    let (Some(rested), Some(cap)) = (record.rested_xp, record.rested_cap()) else {
        return Rested::Unknown {
            why: "no rested XP was captured".to_string(),
        };
    };
    if record.is_resting == Some(false) {
        return Rested::NotResting { rested };
    }
    let fraction = Rested::fraction(rested, cap);
    if fraction >= AT_CAP {
        Rested::AtCap { rested, cap }
    } else if fraction >= NEAR_CAP {
        Rested::NearCap { rested, cap }
    } else {
        Rested::Filling { rested, cap }
    }
}

/// Something a single session could clear. These break ties: given two equally
/// rested characters, play the one where an hour fixes more.
#[derive(Debug, Clone, PartialEq)]
pub struct Friction {
    pub summary: String,
}

fn frictions(record: &CharacterRecord) -> Vec<Friction> {
    let mut found = Vec::new();

    if let Some(quests) = record.quests.as_ref() {
        let capacity = quest_log_capacity(record.game_flavor.as_deref());
        // Within two of the cap is the point at which it starts refusing
        // quests, which is a blocker rather than a tidiness issue.
        if quests.len() + 2 >= capacity {
            found.push(Friction {
                summary: format!(
                    "quest log is {}/{}, so it can take almost nothing new",
                    quests.len(),
                    capacity
                ),
            });
        }
        let complete = quests
            .iter()
            .filter(|quest| quest.state == Some(crate::collector::QuestState::Complete))
            .count();
        if complete > 0 {
            found.push(Friction {
                summary: format!("{complete} quest(s) ready to turn in"),
            });
        }
    }

    if let Some(unspent) = record.talents.as_ref().and_then(|t| t.unspent_points) {
        if unspent > 0 {
            found.push(Friction {
                summary: format!("{unspent} unspent talent point(s)"),
            });
        }
    }

    // A profession at its cap cannot advance until it is trained up, which is a
    // trip to a trainer rather than something that fixes itself while levelling.
    //
    // But the client's skill list is not a list of professions. It also holds
    // talent tab names, weapon skills, armour proficiencies and languages, all
    // of which sit permanently "at cap" and none of which a trainer helps with.
    // Reporting those was the first thing real data caught.
    //
    // The reliable signal is the recipe cache: a skill that appears there was
    // read out of a trade skill window, so it is a trade skill by construction.
    // That is locale-independent, which a list of known profession names would
    // not be. The cost is that a profession whose window has never been opened
    // raises nothing — saying less beats saying something wrong.
    let trade_skills = record.recipes.as_ref();
    for skill in record.skills.iter().flatten() {
        let (Some(name), Some(rank), Some(max)) =
            (skill.name.as_deref(), skill.rank, skill.max_rank)
        else {
            continue;
        };
        let is_trade_skill = trade_skills.is_some_and(|recipes| recipes.contains_key(name));
        if is_trade_skill && max > 0 && rank >= max {
            found.push(Friction {
                summary: format!(
                    "{name} is at its cap ({rank}/{max}) and needs training to go further"
                ),
            });
        }
    }

    if let Some(free) = record.free_bag_slots() {
        if free <= 4 {
            found.push(Friction {
                summary: format!("only {free} free bag slot(s)"),
            });
        }
    }

    found
}

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub key: String,
    pub name: String,
    pub level: Option<u16>,
    pub rested: Rested,
    /// Why this character sits where it does, in terms of its own numbers.
    pub reasons: Vec<String>,
    pub frictions: Vec<Friction>,
}

impl Suggestion {
    /// One line a person can act on.
    pub fn headline(&self) -> String {
        let mut text = self.reasons.join(" ");
        if let Some(first) = self.frictions.first() {
            text.push(' ');
            text.push_str(&format!("Also: {}.", first.summary));
        }
        text
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Advice {
    /// Ranked, most worth playing first. Only characters in the rotation.
    pub play: Vec<Suggestion>,
    /// Not resting, so accruing nothing. The fix is to park them, not play them.
    pub park: Vec<Suggestion>,
    /// Anything that qualifies the advice — an incomplete roster, a client
    /// whose rested model is unknown.
    pub caveats: Vec<String>,
}

impl Advice {
    pub fn play_next(&self) -> Option<&Suggestion> {
        self.play.first()
    }
}

fn describe(rested: &Rested, level: Option<u16>) -> Vec<String> {
    let level_text = level
        .map(|level| format!("Level {level}"))
        .unwrap_or_else(|| "This character".to_string());
    match rested {
        Rested::AtCap { rested, cap } => vec![format!(
            "{level_text}, rested XP is at the cap ({rested} of {cap}), \
             so it has stopped accruing and every hour parked is wasted."
        )],
        Rested::NearCap { rested, cap } => vec![format!(
            "{level_text}, rested XP is {rested} of {cap} ({:.0}%) and will cap soon.",
            Rested::fraction(*rested, *cap) * 100.0
        )],
        Rested::Filling { rested, cap } => vec![format!(
            "{level_text}, rested XP is {rested} of {cap} ({:.0}%) and still filling, \
             so leaving it parked is working.",
            Rested::fraction(*rested, *cap) * 100.0
        )],
        Rested::NotResting { rested } => vec![format!(
            "{level_text}, not in an inn or city, so it is accruing no rest at all \
             (currently {rested}). Park it rather than play it."
        )],
        Rested::Unknown { why } => vec![format!("{level_text}: {why}.")],
    }
}

/// Rank the roster.
///
/// `roster_is_complete` says whether every character is accounted for. When it
/// is not, the advice says so — ranking three of eighteen characters and
/// presenting the winner without qualification is how the prototype managed to
/// be confidently wrong.
pub fn advise(entries: &[RosterEntry<'_>], roster_is_complete: bool) -> Advice {
    let mut advice = Advice::default();

    for entry in entries {
        let rested = rested_state(entry.record);
        let suggestion = Suggestion {
            key: entry.key.to_string(),
            name: entry.name().to_string(),
            level: entry.record.level,
            reasons: describe(&rested, entry.record.level),
            frictions: if entry.role.wants_reminders() {
                frictions(entry.record)
            } else {
                // A character deliberately set down should not be nagged about
                // talent points it is never going to spend.
                Vec::new()
            },
            rested,
        };

        if !entry.role.in_play_rotation() {
            continue;
        }
        if matches!(suggestion.rested, Rested::NotResting { .. }) {
            advice.park.push(suggestion);
        } else {
            advice.play.push(suggestion);
        }
    }

    advice.play.sort_by(|a, b| {
        b.rested
            .weight()
            .cmp(&a.rested.weight())
            // Rested XP is worth more per point at higher level, so among
            // equally capped characters the higher level loses more by waiting.
            .then_with(|| b.level.unwrap_or(0).cmp(&a.level.unwrap_or(0)))
            .then_with(|| b.frictions.len().cmp(&a.frictions.len()))
            .then_with(|| a.name.cmp(&b.name))
    });
    advice.park.sort_by(|a, b| a.name.cmp(&b.name));

    if !roster_is_complete {
        advice.caveats.push(
            "Part of the roster has never been captured, so this ranks only what it can see."
                .to_string(),
        );
    }
    if entries
        .iter()
        .any(|entry| matches!(rested_state(entry.record), Rested::Unknown { .. }))
    {
        advice.caveats.push(
            "Some characters could not be ranked on rest, so they are listed without a position."
                .to_string(),
        );
    }
    if entries.iter().all(|entry| entry.role != Role::Active) && !entries.is_empty() {
        advice
            .caveats
            .push("No character is in the play rotation.".to_string());
    }

    advice
}
