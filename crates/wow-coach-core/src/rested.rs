//! What rested XP does on each client, and how well we know it.
//!
//! The rested model has two numbers and they are known to very different
//! degrees, which is the whole reason this module exists rather than a pair of
//! constants. Keeping them together with their provenance stops a measured
//! number and an assumed one being used as if they were the same thing.
//!
//! - **The rate** — how fast the bar fills while resting.
//! - **The cap** — how full it can get, as a multiple of a level.
//!
//! Only the cap gates the ranking in [`crate::coaching`], because "how much of
//! the cap is left" is what decides whether parked time is being wasted. A
//! known rate with an unknown cap therefore does *not* unblock that ranking,
//! and this module is deliberately shaped so that cannot be fudged.
//!
//! # What was measured on Forever, and what was not
//!
//! Measured on the beta between 26 September and 2 October 2026, one level 1
//! character parked in an inn with `maxXP` 400 and no Legacy points:
//!
//! | elapsed | exhaustion | implied rate | 5% predicts | 4.8% predicts |
//! |---|---|---|---|---|
//! | 2.93 h | 6 | 4.09% of a level per 8 h | 7.3 | 7.0 |
//! | 25.99 h | 62 | 4.77% | 65.0 | 62.4 |
//! | **152.10 h** | **376** | **4.94%** | **380.2** | **365.0** |
//!
//! **The rate is Classic's 5%.** The third reading settles what the first two
//! could not. A flat 4.8% predicts 365 against 376 observed — eleven short,
//! far outside what integer rounding can absorb — while 5% predicts 380, four
//! over.
//!
//! The shape of the error is the evidence. The implied rate climbs 4.09% →
//! 4.77% → 4.94%, approaching 5% from below and never passing it. That is what
//! a fixed loss at the start of a run looks like once the run is long enough to
//! dilute it; a genuinely lower rate would have held steady instead. At a true
//! 5% the whole run is short by **1.7 hours** of accrual, which is why the
//! short readings looked like a slower rate and the long one does not.
//!
//! The 26-hour reading was therefore *not* a measurement of the rate. It was a
//! measurement of the rate plus a startup loss, reported as the rate, and it
//! would have put 4.86% in this file permanently had the longer run not been
//! run. That is the argument for measuring over days rather than hours, and it
//! is the reason this module records sample counts.
//!
//! **The cap is still unmeasured**, and now meaningfully closer. 376 of a
//! possible 600, if Forever keeps Classic's 1.5 levels, is 63% of the way
//! there. Roughly 90 more parked hours would reach it, and the bar flattening
//! is what proves a cap exists — nothing so far has. The Legacy "Well Rested"
//! perk is still expected to move it.
//!
//! # One character per fit
//!
//! The probe does **not** record which character a sample belongs to, and this
//! run nearly paid for it. A second level 2 character on the same account was
//! accruing its own rest, and its sample (exhaustion 150, `maxXP` 900) sits
//! directly beside the one used here in the same file. It was excluded only
//! because `isResting` happened to be false at that instant — luck, not
//! design. Two characters averaged together would have produced a plausible
//! rate that was wrong, with nothing in the output to say so.
//!
//! Until the probe stamps a character key on each sample, every fit must be
//! filtered by `maxXP` and `xp` first. Here that is `maxXP` 400 at 155 XP.
//!
//! The run is archived at `tools/WoWCoachProbe/results/`, and
//! `cargo run -p wow-coach-core --example rested -- <file>` reproduces the fit
//! from it. Read it with that rather than by hand: an earlier ad-hoc reading of
//! the same file silently dropped three samples of twelve, because Lua
//! serialises table keys in no fixed order and the pattern used had anchored on
//! one that was not always first.

/// How a number came to be known. The distinction matters more than the value:
/// a measured rate and an assumed one justify different confidence downstream.
#[derive(Debug, Clone, PartialEq)]
pub enum Basis {
    /// Long-settled behaviour of a shipped client.
    Established,
    /// Fitted from samples this project collected. Carries the count, because
    /// two points and two hundred are not the same claim.
    Measured { samples: usize, note: &'static str },
    /// Not known. Never a default value with a shrug attached.
    Unknown { why: &'static str },
}

impl Basis {
    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Established => "established client behaviour".to_string(),
            Self::Measured { samples, note } => {
                format!("measured from {samples} sample(s): {note}")
            }
            Self::Unknown { why } => why.to_string(),
        }
    }
}

/// The rested model for one client.
#[derive(Debug, Clone, PartialEq)]
pub struct RestedModel {
    /// Fraction of one level gained per 8 hours resting, e.g. 0.05 for 5%.
    pub rate_per_8h: Option<f64>,
    pub rate_basis: Basis,
    /// How full the bar can get, as a multiple of a level.
    pub cap_levels: Option<f64>,
    pub cap_basis: Basis,
}

impl RestedModel {
    /// The cap in XP for a character whose current level needs `max_xp`.
    pub fn cap_xp(&self, max_xp: u64) -> Option<u64> {
        let levels = self.cap_levels?;
        Some((max_xp as f64 * levels) as u64)
    }

    /// XP gained per hour of resting at this level.
    pub fn xp_per_hour(&self, max_xp: u64) -> Option<f64> {
        Some(self.rate_per_8h? * max_xp as f64 / 8.0)
    }

    /// How long from `rested_xp` until the bar stops filling.
    ///
    /// Needs both numbers, which is the point: a client whose cap is unknown
    /// gets `None` here however well its rate is known.
    pub fn hours_to_cap(&self, rested_xp: u64, max_xp: u64) -> Option<f64> {
        let cap = self.cap_xp(max_xp)?;
        let per_hour = self.xp_per_hour(max_xp)?;
        if per_hour <= 0.0 || rested_xp >= cap {
            return Some(0.0);
        }
        Some((cap - rested_xp) as f64 / per_hour)
    }
}

/// Classic and its descendants: both numbers long settled.
fn classic() -> RestedModel {
    RestedModel {
        rate_per_8h: Some(0.05),
        rate_basis: Basis::Established,
        cap_levels: Some(1.5),
        cap_basis: Basis::Established,
    }
}

/// WoW Forever: the rate is measured and matches Classic's; the cap does not.
///
/// Kept as its own function rather than delegating to [`classic`], even though
/// the rate is now the same number. The two are equal by measurement, not by
/// definition — and the cap is now known to *differ*, which is exactly what
/// folding them together would have hidden.
fn forever() -> RestedModel {
    RestedModel {
        rate_per_8h: Some(0.05),
        rate_basis: Basis::Measured {
            samples: 4,
            note: "one level 1 character parked 315 hours in an inn with no Legacy points, \
                   read four times. A flat 4.8% is ruled out: each successive segment reads \
                   closer to 5% per 8 hours without passing it — 4.86%, then 4.98%, then \
                   5.00% across the last 166 hours — which is the signature of about 1.7 \
                   hours of accrual lost at the start rather than of a slower rate. That \
                   last segment is long enough that the startup loss no longer measurably \
                   dilutes it",
        },
        cap_levels: None,
        cap_basis: Basis::Unknown {
            why: "higher than Classic's 1.5 levels, and still not observed. The parked \
                  character passed 600 — Classic's cap for a 400 XP level — around 6 October \
                  and went on filling at the same 5%, reading 790 on 9 October, which is 1.98 \
                  levels, with the accrual still linear to three figures. So the cap is not \
                  1.5 levels here. What it is remains unmeasured, and the Legacy \
                  \"Well Rested\" perk is expected to change it again",
        },
    }
}

/// The model for a client, by the collector's flavor string.
pub fn model_for(flavor: Option<&str>) -> RestedModel {
    match flavor {
        Some("forever") => forever(),
        _ => classic(),
    }
}
