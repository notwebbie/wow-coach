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
    Session::open(&fixture("collector-v3.forever.lua")).expect("the fixture parses")
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

const KEY: &str = "7:forever|16:Classic Beta PvE|9:Trollmage";

fn professions() -> Vec<Value> {
    serde_json::from_str::<Value>(&open().professions_json().unwrap())
        .unwrap()
        .as_array()
        .expect("a list")
        .clone()
}

fn profession(name: &str) -> Value {
    professions()
        .into_iter()
        .find(|plan| plan["profession"] == name)
        .unwrap_or_else(|| panic!("{name} is missing from the professions view"))
}

#[test]
fn every_profession_reaches_the_page_not_only_the_ones_with_recipes() {
    let plans = professions();
    let names: Vec<&str> = plans
        .iter()
        .map(|plan| plan["profession"].as_str().expect("a name"))
        .collect();

    // Herbalism and Cooking have no recipe cache in the fixture. An earlier
    // version of this project decided a gathering profession with no recipes
    // was not worth showing, and a tailor-and-enchanter was reported as having
    // no professions at all. Presence is not conditional on having recipes.
    for expected in ["Alchemy", "Herbalism", "First Aid", "Cooking"] {
        assert!(
            names.contains(&expected),
            "{expected} is missing: {names:?}"
        );
    }
}

#[test]
fn never_looked_and_nothing_left_are_not_the_same_answer() {
    let alchemy = profession("Alchemy");
    assert_eq!(
        alchemy["recipesKnown"], true,
        "the fixture scraped an Alchemy window: {alchemy}"
    );

    let herbalism = profession("Herbalism");
    assert_eq!(
        herbalism["recipesKnown"], false,
        "no Herbalism window was ever opened, so its recipe list is unknown"
    );
    assert_eq!(
        herbalism["skillUps"].as_array().expect("a list").len(),
        0,
        "unknown presents as an empty list, which is exactly why recipesKnown \
         has to travel beside it"
    );
    assert_eq!(
        herbalism["outOfUsefulRecipes"], false,
        "never looked is not out of recipes — the page must not say there is \
         nothing to make when nobody has checked"
    );
}

#[test]
fn a_recipe_with_no_difficulty_is_unknown_and_not_grey() {
    let alchemy = profession("Alchemy");
    let skill_ups = alchemy["skillUps"].as_array().expect("a list");

    // "Recipe From A Later Client" carries no difficulty in the fixture. Grey
    // means it will never skill up; unknown means the client did not say. The
    // first is advice, the second is a gap, and they must not share a colour.
    let difficulties: Vec<&str> = skill_ups
        .iter()
        .map(|entry| entry["difficulty"].as_str().expect("a difficulty"))
        .collect();
    assert!(
        !difficulties.contains(&"trivial"),
        "grey recipes cannot raise the skill and do not belong in skillUps: \
         {difficulties:?}"
    );
    assert!(
        difficulties.contains(&"optimal"),
        "the orange recipe is the one worth making: {difficulties:?}"
    );
}

#[test]
fn training_says_which_gate_is_shut_not_merely_that_one_is() {
    let alchemy = profession("Alchemy");
    let training = &alchemy["training"];

    // 225 of 300 is short of the cap, so no trainer is involved yet. The
    // state is still named rather than reported as a bare false, because
    // "not yet" and "the trainer wants more skill" are different advice.
    assert_eq!(training["state"], "notYet", "{training}");
    assert!(
        training.get("needs").is_some(),
        "the field is always present, null when it does not apply: {training}"
    );
}

#[test]
fn professions_reach_the_page_in_camel_case() {
    let alchemy = profession("Alchemy");
    assert!(
        alchemy.get("maxRank").is_some() && alchemy.get("max_rank").is_none(),
        "JavaScript reads camelCase: {alchemy}"
    );
    assert!(
        alchemy.get("recipesKnown").is_some(),
        "and so does the flag that stops absent being read as empty: {alchemy}"
    );
}

#[test]
fn a_completed_quest_is_a_turn_in_and_the_zone_is_named() {
    let plan: Value = serde_json::from_str(&open().quests_json(KEY).unwrap()).unwrap();

    let turn_ins = plan["turnIns"].as_array().expect("a list");
    assert_eq!(turn_ins.len(), 1, "one quest is complete in the fixture");
    assert_eq!(turn_ins[0]["title"], "Cutting Teeth");

    assert_eq!(
        plan["bestZone"], "Durotar",
        "the core picks the zone; the page does not re-derive it: {plan}"
    );

    let log = &plan["log"];
    for field in ["used", "capacity", "free", "isTight"] {
        assert!(log.get(field).is_some(), "{field} is missing from {log}");
    }
}

#[test]
fn a_quest_carries_how_far_out_of_reach_it_is() {
    let plan: Value = serde_json::from_str(&open().quests_json(KEY).unwrap()).unwrap();
    let cluster = &plan["clusters"].as_array().expect("a list")[0];

    assert_eq!(
        cluster["zone"], "Durotar",
        "clusters use the client's own zone headers: {cluster}"
    );
    assert!(
        cluster.get("worthATrip").is_some(),
        "the reason to go travels with the zone: {cluster}"
    );

    // Trollmage is 24 and the quests are levels 3 and 4, so both are trivial
    // by a distance. The distance is the useful part: "trivial" alone does not
    // tell a player whether to bother.
    let trivial = cluster["trivial"].as_array().expect("a list");
    assert!(
        !trivial.is_empty(),
        "level 3 at level 24 is grey: {cluster}"
    );
    assert_eq!(
        trivial[0]["standing"], "trivial",
        "the standing is named: {}",
        trivial[0]
    );
    assert!(
        trivial[0]["by"].as_u64().is_some(),
        "and carries the gap: {}",
        trivial[0]
    );
}

#[test]
fn asking_for_a_character_that_is_not_there_names_the_key() {
    let error = open()
        .quests_json("7:forever|16:Classic Beta PvE|6:Nobody")
        .unwrap_err();
    assert!(
        error.contains("Nobody"),
        "a mistyped key is a bug in the page, and silence would hide it: {error}"
    );
}

/// A collector file written by hand, so a case the committed fixture does not
/// contain can be stated exactly.
fn one_character(level: u16, quests: &str) -> Session {
    Session::open(&format!(
        r#"WoWCoachCollectorDB = {{
    ["schemaVersion"] = 2,
    ["characters"] = {{
        ["7:forever|16:Classic Beta PvE|6:Solitu"] = {{
            ["capturedAt"] = 1700000000,
            ["class"] = "MAGE",
            ["faction"] = "Horde",
            ["gameFlavor"] = "forever",
            ["level"] = {level},
            ["name"] = "Solitu",
            ["realm"] = "Classic Beta PvE",
            ["quests"] = {{{quests}}},
            ["questsComplete"] = true,
        }},
    }},
}}"#
    ))
    .expect("the hand-written file parses")
}

#[test]
fn a_zone_with_nothing_worth_doing_is_not_the_zone_to_go_to() {
    // The committed fixture's only zone happens to hold a completed quest, so
    // it is both the first cluster and a worthwhile one. That makes "the best
    // zone" and "the first zone" indistinguishable there, and a binding that
    // read `clusters.first()` instead of `best_zone()` passed every other test
    // in this file. Hence a character at 24 whose only quest is a grey level 3:
    // the cluster exists, and it is still not a reason to travel.
    let session = one_character(
        24,
        r#"{ ["header"] = "Durotar", ["level"] = 3, ["questID"] = 788,
             ["state"] = "incomplete", ["title"] = "Cutting Teeth" },"#,
    );
    let plan: Value = serde_json::from_str(
        &session
            .quests_json("7:forever|16:Classic Beta PvE|6:Solitu")
            .unwrap(),
    )
    .unwrap();

    assert_eq!(
        plan["clusters"].as_array().expect("a list").len(),
        1,
        "the zone is still reported, because the quest is still in the log: {plan}"
    );
    assert_eq!(
        plan["clusters"][0]["worthATrip"], 0,
        "nothing there can be cleared: {plan}"
    );
    assert!(
        plan["bestZone"].is_null(),
        "so there is nowhere worth going, and the page must not be handed a \
         zone name it would print as advice: {plan}"
    );
}

#[test]
fn a_quest_carries_its_id_so_a_client_can_link_to_it() {
    let plan: Value = serde_json::from_str(&open().quests_json(KEY).unwrap()).unwrap();
    let turn_in = &plan["turnIns"].as_array().expect("a list")[0];

    assert_eq!(
        turn_in["questId"], 788,
        "the id the client gave the quest travels with the title: {turn_in}"
    );
}

#[test]
fn a_quest_with_no_captured_id_still_reaches_the_page() {
    // The collector writes what the client reported and nothing else, so a
    // record without questID is a real shape. The quest must still be listed —
    // the page can render a title without a link, but it cannot render a quest
    // it was never handed.
    let session = one_character(
        12,
        r#"{ ["header"] = "Durotar", ["level"] = 12,
             ["state"] = "complete", ["title"] = "A Quest With No Id" },"#,
    );
    let plan: Value = serde_json::from_str(
        &session
            .quests_json("7:forever|16:Classic Beta PvE|6:Solitu")
            .unwrap(),
    )
    .unwrap();

    let turn_ins = plan["turnIns"].as_array().expect("a list");
    assert_eq!(turn_ins.len(), 1, "the quest is not dropped: {plan}");
    assert_eq!(turn_ins[0]["title"], "A Quest With No Id");
    assert!(
        turn_ins[0]["questId"].is_null(),
        "and the absent id is null rather than a zero the page would link to: {}",
        turn_ins[0]
    );
}

#[test]
fn gear_reaches_the_page_with_the_wand_question_answered() {
    // The Forever fixture equips a wand, so the page should be told it is met
    // rather than handed a list of things to go and buy.
    let plan: Value = serde_json::from_str(&open().gear_json(KEY).unwrap()).unwrap();

    assert_eq!(plan["equipmentKnown"], true, "the fixture has a wardrobe");

    // One rung per ladder reaches the page. The fixture mage is 24 carrying an
    // item-level 14 wand, so the rung on offer is the upgrade, not the starter.
    let wand: Vec<&Value> = plan["milestones"]
        .as_array()
        .expect("a list")
        .iter()
        .filter(|m| m["line"] == "wand")
        .collect();
    assert_eq!(wand.len(), 1, "one rung, not two: {wand:#?}");
    assert_eq!(wand[0]["what"], "A better wand", "{}", wand[0]);
    assert_eq!(wand[0]["state"], "due", "{}", wand[0]);
}

#[test]
fn an_unchecked_claim_stays_unchecked_all_the_way_to_the_page() {
    // The binding layer is the easiest place for a caveat to be quietly
    // dropped: the Rust side is honest, the JSON is not, and nobody notices
    // because both halves look fine on their own.
    let plan: Value = serde_json::from_str(&open().gear_json(KEY).unwrap()).unwrap();
    for milestone in plan["milestones"].as_array().expect("a list") {
        assert!(
            milestone
                .get("source")
                .is_some_and(|s| !s.as_str().unwrap_or("").is_empty()),
            "every milestone names its source on the way out: {milestone}"
        );
        assert!(
            milestone.get("checkedForForever").is_some(),
            "and says whether anyone checked it against this game: {milestone}"
        );
    }
}

#[test]
fn gear_for_a_character_that_is_not_there_names_the_key() {
    let error = open()
        .gear_json("7:forever|16:Classic Beta PvE|6:Nobody")
        .unwrap_err();
    assert!(error.contains("Nobody"), "{error}");
}
