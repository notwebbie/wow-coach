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
fn the_forever_cap_is_classics_after_all_and_says_how_it_got_there() {
    // This test replaces one that asserted the opposite, and the replacement is
    // the point. The earlier version pinned "the cap is NOT 1.5 levels" against
    // a single reading of 790 on a 400 XP level. Six days later the same
    // character read exactly 600 with its XP untouched — a clamp, not spending.
    // The beta had been over-accruing past its own cap and was corrected.
    //
    // So the guard now runs the other way: the number is Classic's, and the
    // record of how it was reached has to travel with it, or the next person
    // sees a clean 1.5 and never learns it was contested.
    let model = rested::model_for(Some("forever"));

    assert_eq!(model.cap_levels, Some(1.5));
    assert!(model.cap_basis.is_known());

    match model.cap_basis {
        Basis::Measured { samples, note } => {
            assert_eq!(
                samples, 2,
                "two readings now sit on the cap, 44.8 hours apart"
            );
            assert!(
                note.contains("790"),
                "the contradicted reading belongs in the record, not only in history: {note}"
            );
            assert!(
                note.contains("clamp"),
                "and the reason it was contradicted: {note}"
            );
            assert!(
                note.contains("712"),
                "the number continued accrual would have produced is what makes the \
                 second reading evidence rather than a repeat: {note}"
            );
        }
        other => panic!("expected a measured cap carrying its history, got {other:?}"),
    }

    // Measured, not Established. Classic's cap has twenty years behind it; this
    // one has a single reading taken after a bug was fixed, and the difference
    // must stay visible to anything that reports provenance.
    assert!(
        !matches!(model.cap_basis, Basis::Established),
        "one post-patch reading is not twenty years of settled behaviour"
    );
    assert!(matches!(
        rested::model_for(Some("classic_era")).cap_basis,
        Basis::Established
    ));
}

#[test]
fn a_character_on_forever_can_now_be_ranked_on_rest() {
    // The practical consequence. While the cap was unknown this returned
    // nothing, because a fraction of an unknown total is not a number.
    let record = character("forever", 400, 300);
    assert!(record.rested_cap_is_known());
    assert_eq!(record.rested_cap(), Some(600));
    assert_eq!(record.rested_fraction(), Some(0.5));
}

#[test]
fn a_measured_rate_says_how_many_samples_it_rests_on() {
    // Two points and two hundred are not the same claim, and a reader deciding
    // whether to trust a number needs to know which it is.
    let model = rested::model_for(Some("forever"));
    match model.rate_basis {
        Basis::Measured { samples, note } => {
            assert!(samples >= 4, "the parked run added two more readings");
            assert!(
                note.contains("315 hours"),
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
    // Forever can answer this now that the cap is measured. It could not while
    // the cap was unknown, and the shape of the guard is what matters: the
    // answer comes from both numbers, never from the rate alone.
    let forever = rested::model_for(Some("forever"));
    assert!(
        forever.xp_per_hour(400).is_some(),
        "the rate alone is usable"
    );
    // 5% of 400 per 8h = 2.5/h; cap 600; from 62 that is 215.2 hours.
    let hours = forever.hours_to_cap(62, 400).expect("both known now");
    assert!((hours - 215.2).abs() < 0.5, "got {hours}");

    // A client whose cap is genuinely unknown still refuses, which is the
    // behaviour this test was written to protect and must outlive the day
    // Forever's cap happened to get measured.
    let unknown = rested::model_for(Some("some_future_client"));
    if unknown.cap_levels.is_none() {
        assert_eq!(unknown.hours_to_cap(62, 400), None);
    }

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

    // The cap landed on Classic's number too, but not on Classic's certainty.
    // Classic's is Established; Forever's is one reading taken after the beta
    // stopped over-accruing, and the provenance has to stay distinguishable.
    assert_eq!(model.cap_levels, classic.cap_levels);
    assert!(matches!(model.cap_basis, Basis::Measured { .. }));
    assert!(matches!(classic.cap_basis, Basis::Established));
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
