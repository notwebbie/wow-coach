//! Read the probe's rested samples through the project's own Lua parser.
//!
//! This exists because the alternative got it wrong. Reading these files with
//! a regex looked fine for a week and then quietly lost three samples out of
//! twelve, because Lua serialises a table's keys in whatever order it likes
//! and the pattern happened to anchor on a key that was not always first.
//! Nothing in the output said so — it just reported eight samples as if that
//! were all of them, and the conclusion drawn was that the game had discarded
//! data it had not touched.
//!
//! The repository already contains a parser that cannot make that mistake.
//! Using it is the point: the same code that reads a collector file in
//! production reads the diagnostic here, so the analysis and the product
//! cannot disagree about what a file says.
//!
//! ```sh
//! cargo run -p wow-coach-core --example rested -- <WoWCoachProbe.lua>
//! ```

use wow_coach_core::lua;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: rested <WoWCoachProbe.lua>");
    let text = std::fs::read_to_string(&path).expect("read the probe file");
    let globals = lua::parse_saved_variables(&text).expect("parse");
    let db = globals
        .get("WoWCoachProbeDB")
        .expect("WoWCoachProbeDB is not in this file — is it the probe's saved variables?");
    let json = lua::to_json(db).expect("to json");

    let samples = json
        .get("restedSamples")
        .and_then(|value| value.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default();

    println!("{} rested sample(s)", samples.len());

    let field = |sample: &serde_json::Value, key: &str| -> String {
        match sample.get(key) {
            Some(serde_json::Value::Null) | None => "-".to_string(),
            Some(value) => value.to_string().trim_matches('"').to_string(),
        }
    };

    let mut previous: Option<i64> = None;
    let mut first_resting: Option<(i64, f64)> = None;
    let mut last_resting: Option<(i64, f64)> = None;

    for sample in samples {
        let at = sample
            .get("at")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let gap = match previous {
            Some(before) if at > before => {
                let seconds = at - before;
                format!("+{}h{:02}m", seconds / 3600, (seconds % 3600) / 60)
            }
            _ => String::new(),
        };
        previous = Some(at);

        // A sample carrying values forward from an earlier one is not an
        // observation and must never anchor a fit.
        let carried = sample
            .get("valuesAreStale")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let exhaustion = sample.get("exhaustion").and_then(serde_json::Value::as_f64);
        let resting = sample
            .get("isResting")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if let (Some(exhaustion), true, false) = (exhaustion, resting, carried) {
            if first_resting.is_none() {
                first_resting = Some((at, exhaustion));
            }
            last_resting = Some((at, exhaustion));
        }

        println!(
            "  {at:>11} {gap:>9}  {:<22} exh={:<5} rest={:<6} st={:<4} xp={}/{}{}",
            field(sample, "reason"),
            field(sample, "exhaustion"),
            field(sample, "isResting"),
            field(sample, "restState"),
            field(sample, "xp"),
            field(sample, "maxXP"),
            if carried { "  CARRIED" } else { "" },
        );
    }

    // The fit, between the first and last genuine resting observations.
    if let (Some((from, low)), Some((to, high))) = (first_resting, last_resting) {
        let hours = (to - from) as f64 / 3600.0;
        if hours > 0.0 && high > low {
            let max_xp = samples
                .iter()
                .filter_map(|sample| sample.get("maxXP").and_then(serde_json::Value::as_f64))
                .find(|max| *max > 0.0)
                .unwrap_or(0.0);
            let per_hour = (high - low) / hours;
            println!();
            println!("  {hours:.2} h of resting, exhaustion {low} -> {high}");
            println!("  {per_hour:.3} XP/hour");
            if max_xp > 0.0 {
                println!(
                    "  {:.2}% of a level per 8 hours (maxXP {max_xp})",
                    per_hour * 8.0 / max_xp * 100.0
                );
            }
        }
    }
}
