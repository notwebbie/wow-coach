//! Tests for the browser bindings.
//!
//! The claim under test: this layer reshapes and forwards, and never decides
//! anything. The shapes are checked against the committed fixture, because a
//! renamed field here reaches the page as a blank where a number should be and
//! nothing else would catch it.

use serde_json::Value;
use wow_coach_wasm::Session;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture")
}

fn open() -> Session {
    Session::open(&fixture("collector-v2.forever.lua")).expect("the fixture parses")
}

#[test]
fn a_file_the_parser_rejects_says_so_rather_than_returning_nothing() {
    // The addon owns the writer and the core owns the reader, so a file
    // outside the subset means the collector has a bug. Recovering something
    // plausible would hide it.
    let error = Session::open("WoWCoachCollectorDB = function() end").unwrap_err();
    assert!(
        error.contains("could not read that file"),
        "the failure must name itself: {error}"
    );
}

#[test]
fn characters_carry_what_the_first_screen_needs() {
    let characters: Value = serde_json::from_str(&open().characters_json().unwrap()).unwrap();
    let first = &characters.as_array().expect("a list")[0];

    // Recognition is the whole job of this screen: the player sees their own
    // characters and believes the tool read the game.
    for field in ["key", "name", "level", "class", "realm", "flavor", "role"] {
        assert!(
            first.get(field).is_some(),
            "{field} is missing from the roster view: {first}"
        );
    }
    assert_eq!(
        first["role"], "unset",
        "a character nobody has classified is unset, not active"
    );
    // camelCase crosses the boundary, because JavaScript reads it.
    assert!(
        first.get("capturedAt").is_some() && first.get("captured_at").is_none(),
        "fields must reach the page in camelCase: {first}"
    );
}

#[test]
fn nothing_is_recommended_until_the_player_says_what_they_play() {
    // The first-run state, and not an error. An empty answer here means the
    // page should ask the question, not apologise.
    let view: Value = serde_json::from_str(&open().recommendation_json().unwrap()).unwrap();
    assert!(view["playNext"].is_null());
    assert!(
        view["caveats"]
            .as_array()
            .unwrap()
            .iter()
            .any(|caveat| caveat.as_str().unwrap().contains("Nothing is marked")),
        "and it must say what is missing: {view}"
    );
}

#[test]
fn ticking_a_character_produces_one_answer_with_its_sentence() {
    let mut session = open();
    let characters: Value = serde_json::from_str(&session.characters_json().unwrap()).unwrap();
    let key = characters[0]["key"].as_str().unwrap().to_string();

    session
        .apply_roles(&format!(r#"{{"{key}":"active"}}"#))
        .expect("a real key and a real role");

    let view: Value = serde_json::from_str(&session.recommendation_json().unwrap()).unwrap();
    let next = &view["playNext"];
    assert!(!next.is_null(), "a character in the rotation gets ranked");
    assert_eq!(next["key"].as_str().unwrap(), key);

    // The finished sentence is built in the core so every client says the same
    // thing. The page displays it; it must not have to assemble it.
    let headline = next["headline"].as_str().expect("a headline");
    assert!(
        !headline.is_empty(),
        "the recommendation must arrive as a sentence, not as parts"
    );

    // And the rest of the ranking sits underneath rather than competing.
    assert!(view["then"].is_array());
}

#[test]
fn roles_are_all_applied_or_none_are() {
    // A typo in the fourth entry must not leave the first three written. The
    // page sends the whole set from a form, so a partial apply would show a
    // state the player never chose.
    let mut session = open();
    let characters: Value = serde_json::from_str(&session.characters_json().unwrap()).unwrap();
    let key = characters[0]["key"].as_str().unwrap().to_string();

    let error = session
        .apply_roles(&format!(r#"{{"{key}":"active","nobody":"bank"}}"#))
        .unwrap_err();
    assert!(
        error.contains("nobody"),
        "it must name the bad key: {error}"
    );

    let after: Value = serde_json::from_str(&session.characters_json().unwrap()).unwrap();
    assert_eq!(
        after[0]["role"], "unset",
        "the valid half of a rejected set must not have been applied"
    );
}

#[test]
fn a_word_that_is_not_a_role_is_refused_by_name() {
    let mut session = open();
    let characters: Value = serde_json::from_str(&session.characters_json().unwrap()).unwrap();
    let key = characters[0]["key"].as_str().unwrap();
    let error = session
        .apply_roles(&format!(r#"{{"{key}":"levelling"}}"#))
        .err();
    assert!(
        error.is_none(),
        "the words players actually use are roles: {error:?}"
    );

    let error = session
        .apply_roles(&format!(r#"{{"{key}":"maybe"}}"#))
        .unwrap_err();
    assert!(
        error.contains("maybe"),
        "and anything else is named: {error}"
    );
}

#[test]
fn parse_notices_reach_the_page_rather_than_being_swallowed() {
    let notices: Value = serde_json::from_str(&open().notices_json().unwrap()).unwrap();
    assert!(notices.is_array(), "always a list, even when empty");
}
