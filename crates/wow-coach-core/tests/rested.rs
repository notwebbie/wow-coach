//! Tests for the rested model.
//!
//! The claim under test: a measured rate and an unmeasured cap are different
//! things, and knowing one must never be allowed to pass for knowing the other.

use wow_coach_core::collector::CharacterRecord;
use wow_coach_core::rested::{self, Basis};

fn character(flavor: &str, max_xp: u64, rested_xp: u64) -> CharacterRecord {
    CharacterRecord {
        name: Some("Test".to_string()),
        level: Some(1),
        game_flavor: Some(flavor.to_string()),
        max_xp: Some(max_xp),
        rested_xp: Some(rested_xp),
        is_resting: Some(true),
        ..Default::default()
    }
}

#[test]
fn measuring_the_rate_does_not_unlock_the_cap() {
    // The headline. Forever's fill rate was measured over 26 hours in an inn,
    // but nothing observed bears on where the bar stops — the longest run
    // reached a tenth of Classic's cap. The ranking needs the cap, so it must
    // still refuse to rank on rest.
    let model = rested::model_for(Some("forever"));

    assert!(model.rate_per_8h.is_some(), "the rate is known");
    assert!(model.rate_basis.is_known());
    assert!(matches!(model.rate_basis, Basis::Measured { .. }));

    assert!(model.cap_levels.is_none(), "the cap is not");
    assert!(!model.cap_basis.is_known());

    let record = character("forever", 400, 62);
    assert!(
        !record.rested_cap_is_known(),
        "and a character on it still cannot be ranked on rest"
    );
    assert_eq!(record.rested_cap(), None);
    assert_eq!(record.rested_fraction(), None);
}

#[test]
fn a_measured_rate_says_how_many_samples_it_rests_on() {
    // Two points and two hundred are not the same claim, and a reader deciding
    // whether to trust a number needs to know which it is.
    let model = rested::model_for(Some("forever"));
    match model.rate_basis {
        Basis::Measured { samples, note } => {
            assert!(samples >= 4, "the 152 hour run added two more readings");
            assert!(
                note.contains("152 hours"),
                "the length of the run is what makes the number trustworthy"
            );
            assert!(
                note.contains("4.8%"),
                "the hypothesis this run ruled out belongs in the record, not only in \
                 the commit message — otherwise the next person re-opens a settled question"
            );
        }
        other => panic!("expected a measured rate, got {other:?}"),
    }
}

#[test]
fn classic_keeps_the_cap_it_has_always_had() {
    let model = rested::model_for(Some("tbc_classic"));
    assert_eq!(model.cap_levels, Some(1.5));
    assert_eq!(model.rate_per_8h, Some(0.05));
    assert!(model.cap_basis.is_known());

    // Level 20 needs about 20,800 XP, so the cap is 31,200 — the figure the
    // live roster reported for a capped character.
    assert_eq!(model.cap_xp(20_800), Some(31_200));

    let record = character("tbc_classic", 20_800, 31_200);
    assert!(record.rested_cap_is_known());
    assert_eq!(record.rested_cap(), Some(31_200));
    assert_eq!(record.rested_fraction(), Some(1.0));
}

#[test]
fn hours_to_cap_needs_both_numbers_not_just_the_rate() {
    // Forever knows how fast the bar fills and not where it stops, so it
    // cannot say when filling ends. Returning a figure from the rate alone
    // would be inventing the cap.
    let forever = rested::model_for(Some("forever"));
    assert!(
        forever.xp_per_hour(400).is_some(),
        "the rate alone is usable"
    );
    assert_eq!(
        forever.hours_to_cap(62, 400),
        None,
        "but not for a question that needs the cap"
    );

    let classic = rested::model_for(Some("tbc_classic"));
    // 5% of 20,800 per 8h = 130/h; cap 31,200; from zero that is 240 hours.
    let hours = classic.hours_to_cap(0, 20_800).expect("both known");
    assert!((hours - 240.0).abs() < 0.5, "got {hours}");
    assert_eq!(
        classic.hours_to_cap(31_200, 20_800),
        Some(0.0),
        "already capped is zero hours, not a negative"
    );
}

#[test]
fn the_forever_rate_matches_the_long_run_not_the_short_one() {
    // Guards the number against a careless edit, and against the earlier
    // conclusion creeping back. 152 hours of resting at this rate must
    // reproduce roughly the 376 exhaustion measured, at the level 1 maxXP of
    // 400 it was measured at.
    let model = rested::model_for(Some("forever"));
    let per_hour = model.xp_per_hour(400).expect("rate known");

    let after_152h = per_hour * 152.10;
    assert!(
        (after_152h - 376.0).abs() < 6.0,
        "152 hours should land near the observed 376, got {after_152h}"
    );

    // The hypothesis this run killed. A flat 4.8% predicts 365 over the same
    // span, eleven short of what was seen, so the model must not drift back
    // toward it.
    let flat_48: f64 = 0.048 * 400.0 / 8.0 * 152.10;
    assert!(
        (after_152h - 376.0).abs() < (flat_48 - 376.0).abs(),
        "the model must fit the measurement better than the rate it ruled out"
    );

    // It landed on Classic's rate. Equal by measurement rather than by
    // definition, which is why the two models stay separate functions.
    let classic = rested::model_for(Some("tbc_classic"));
    assert_eq!(
        model.rate_per_8h, classic.rate_per_8h,
        "the measurement came in at Classic's 5%"
    );

    // But not Classic's certainty: the cap is still unobserved here, and that
    // is what gates the ranking.
    assert!(model.cap_levels.is_none());
    assert!(!model.cap_basis.is_known());
    assert!(classic.cap_basis.is_known());
}

#[test]
fn an_unknown_client_is_treated_as_classic_rather_than_as_nothing() {
    // A flavor this build predates is far more likely to be another Classic
    // descendant than something novel, and refusing to rank it at all would
    // be worse than ranking it on the family's long-settled numbers.
    let model = rested::model_for(Some("some_future_classic"));
    assert_eq!(model.cap_levels, Some(1.5));
    assert!(model.cap_basis.is_known());
}
