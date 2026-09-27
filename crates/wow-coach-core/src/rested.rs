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
//! Measured on the beta, 26–27 September 2026, one level 1 character parked in
//! an inn with `maxXP` 400 and no Legacy points:
//!
//! | elapsed | exhaustion | implied rate |
//! |---|---|---|
//! | 2.93 h | 6 | 4.09% of a level per 8 h |
//! | 25.99 h | 62 | 4.77% |
//!
//! The second reading is the trustworthy one — twenty-six hours of accrual
//! against integers that round to under a percent, where the first spans three
//! hours and six whole numbers. Between the two readings the rate works out at
//! **4.86%**, and the pair fit exactly if accrual is 4.86% per 8 h starting
//! about 28 minutes after the character begins resting.
//!
//! So two explanations survive, and they are not distinguished by this data:
//! a flat rate near **4.8%** of a level per 8 hours, or Classic's **5%** with a
//! startup delay. Classic's 5% with no delay predicts 65 at 26 hours against
//! the 62 observed, which is further out than rounding allows.
//!
//! A longer parked run separates them: a startup delay is a fixed subtraction
//! and washes out over days, where a lower rate does not. Until then the value
//! here is the measurement, not the mechanic.
//!
//! **The cap is still unmeasured.** 62 of a possible 600, if Forever keeps
//! Classic's 1.5 levels, is a tenth of the way there. Nothing observed so far
//! bears on where the bar stops, and the Legacy "Well Rested" perk is expected
//! to move it.
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

/// WoW Forever: the rate is measured, the cap is not.
fn forever() -> RestedModel {
    RestedModel {
        rate_per_8h: Some(0.0486),
        rate_basis: Basis::Measured {
            samples: 2,
            note: "one level 1 character parked 26 hours in an inn with no Legacy points; \
                   consistent with a flat 4.8% or with Classic's 5% after a ~28 minute \
                   startup delay, which this data cannot separate",
        },
        cap_levels: None,
        cap_basis: Basis::Unknown {
            why: "the rested cap has never been observed on this client — the longest run \
                  reached a tenth of Classic's cap — and the Legacy \"Well Rested\" perk is \
                  expected to change it",
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
