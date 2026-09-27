//! Show what the collector captured for each profession, per character.
//!
//! Companion to `examples/rested.rs`, and it exists for the same reason: the
//! alternative is a regex over a Lua file, which has now produced a wrong
//! answer twice. Lua serialises a table's keys in no fixed order, so any
//! pattern that assumes one silently sees less than is there — and reports the
//! shortfall as a finding rather than as a bug.
//!
//! This goes through the typed loader, so it reads the file exactly as the
//! product does. If it disagrees with `wow-coach craft`, the disagreement is
//! real rather than an artifact of how the file was read.
//!
//! ```sh
//! cargo run -p wow-coach-core --example recipes -- <WoWCoachCollector.lua>
//! ```

use wow_coach_core::collector;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: recipes <WoWCoachCollector.lua>");
    let text = std::fs::read_to_string(&path).expect("read the collector file");
    let loaded = collector::load(&text).expect("parse");

    for notice in &loaded.notices {
        eprintln!("note: {notice}");
    }

    let mut total_recipes = 0usize;
    let mut with_reagents = 0usize;

    for (key, record) in &loaded.db.characters {
        let Some(recipes) = record.recipes.as_ref().filter(|map| !map.is_empty()) else {
            continue;
        };
        println!(
            "{} — {}",
            record.display_name(key),
            record.game_flavor.as_deref().unwrap_or("unknown client")
        );

        for (profession, cache) in recipes {
            let entries = cache.recipes.as_deref().unwrap_or(&[]);
            let reagented = entries
                .iter()
                .filter(|recipe| {
                    recipe
                        .reagents
                        .as_ref()
                        .is_some_and(|list| !list.is_empty())
                })
                .count();
            let makes = entries
                .iter()
                .filter(|recipe| recipe.makes_item_id.is_some())
                .count();
            total_recipes += entries.len();
            with_reagents += reagented;

            println!(
                "  {:<16} rank {:>3}/{:<3}  {} recipe(s), {} with reagents, {} producing an item",
                profession,
                cache
                    .rank
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "?".into()),
                cache
                    .max_rank
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "?".into()),
                entries.len(),
                reagented,
                makes,
            );

            for recipe in entries {
                let name = recipe.name.as_deref().unwrap_or("(unnamed)");
                let reagents = match recipe.reagents.as_deref() {
                    Some(list) if !list.is_empty() => list
                        .iter()
                        .map(|reagent| {
                            format!(
                                "{}x{}",
                                reagent
                                    .item_id
                                    .map(|id| id.to_string())
                                    .unwrap_or_else(|| "?".into()),
                                reagent.count.unwrap_or(0)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" + "),
                    // Absent reagents mean "not captured", never "free". A
                    // reader that printed nothing here would look the same as
                    // one printing a recipe that genuinely needs nothing.
                    _ => "NO REAGENTS CAPTURED".to_string(),
                };
                let output = match recipe.makes_item_id {
                    Some(item) => format!(
                        "-> {item} x{}",
                        recipe
                            .makes_typical()
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "?".into())
                    ),
                    // An enchant is applied to gear and produces nothing. That
                    // is a fact about the recipe, not a gap in the capture.
                    None => "-> no item".to_string(),
                };
                println!("      {name:<38} {reagents:<32} {output}");
            }
        }
        println!();
    }

    println!("{total_recipes} recipe(s) in total, {with_reagents} with reagents");
    if total_recipes > with_reagents {
        println!(
            "{} were captured before reagents were recorded — open those windows again",
            total_recipes - with_reagents
        );
    }
}
