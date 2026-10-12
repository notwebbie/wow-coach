//! Tests for the talent planner.
//!
//! The warlock tree here is the Forever Affliction layout as the live client
//! drew it on 13 October 2026: Improved Corruption maxed in row 1, one point
//! in row 2's third column, and four unspent at level 19. The question it
//! has to answer is the one the planner exists for — where do those four go?

use std::collections::BTreeMap;

use wow_coach_core::collector::{
    self, CharacterRecord, Talent, TalentPosition, TalentTree, Talents,
};
use wow_coach_core::talents::{
    self, Blocked, BuildStep, PlanError, SpecChoice, TalentBuild, POINTS_PER_TIER,
};

fn talent(name: &str, tier: u8, column: u8, rank: u8, max: u8) -> Talent {
    Talent {
        name: Some(name.into()),
        tier: Some(tier),
        column: Some(column),
        rank: Some(rank),
        max_rank: Some(max),
        ..Default::default()
    }
}

fn tree(name: &str, talents: Vec<Talent>) -> TalentTree {
    let spent = talents.iter().map(|t| u16::from(t.rank.unwrap_or(0))).sum();
    TalentTree {
        name: Some(name.into()),
        points_spent: Some(spent),
        talents: Some(talents),
    }
}

fn warlock(trees: Vec<TalentTree>, unspent: u16) -> CharacterRecord {
    CharacterRecord {
        name: Some("Examplelock".into()),
        level: Some(19),
        class: Some("WARLOCK".into()),
        game_flavor: Some("forever".into()),
        talents: Some(Talents {
            trees: Some(trees),
            unspent_points: Some(unspent),
        }),
        ..Default::default()
    }
}

fn affliction(improved_corruption: u8, malediction: u8, suppression: u8) -> TalentTree {
    let mut siphon = talent("Siphon Life", 5, 2, 0, 1);
    siphon.requires = None;
    let mut mastery = talent("Shadow Mastery", 6, 2, 0, 5);
    mastery.requires = Some(TalentPosition { tier: 5, column: 2 });
    tree(
        "Affliction",
        vec![
            talent("Improved Life Tap", 1, 1, 0, 2),
            talent("Suppression", 1, 2, suppression, 5),
            talent("Improved Corruption", 1, 3, improved_corruption, 5),
            talent("Improved Drain Soul", 2, 1, 0, 2),
            talent("Soul Siphon", 2, 2, 0, 2),
            talent("Malediction", 2, 3, malediction, 5),
            talent("Improved Bane of Agony", 3, 1, 0, 2),
            talent("Fel Concentration", 3, 2, 0, 5),
            talent("Amplify Curse", 3, 3, 0, 1),
            talent("Pandemic", 3, 4, 0, 3),
            talent("Nightfall", 4, 1, 0, 2),
            siphon,
            mastery,
        ],
    )
}

fn builds() -> Vec<TalentBuild> {
    talents::builtin_builds()
}

fn forever_affliction() -> TalentBuild {
    builds()
        .into_iter()
        .find(|b| b.id == "forever-warlock-affliction")
        .unwrap()
}

#[test]
fn the_four_points_at_nineteen_go_into_suppression() {
    let record = warlock(vec![affliction(5, 1, 0), tree("Demonology", vec![])], 4);
    let plan = talents::plan(&record, &forever_affliction(), 5).unwrap();

    assert_eq!(plan.unspent, 4);
    assert_eq!(plan.now.len(), 1, "{:?}", plan.now);
    assert_eq!(plan.now[0].talent, "Suppression");
    assert_eq!((plan.now[0].from_rank, plan.now[0].to_rank), (0, 4));
    assert_eq!((plan.now[0].tier, plan.now[0].column), (1, 2));
    assert!(plan.blocked.is_none());
}

#[test]
fn steps_already_done_are_skipped_and_the_walk_carries_on() {
    // Improved Corruption and the first Malediction point are done, so after
    // Suppression the next thing is row 3 — open at ten points.
    let record = warlock(vec![affliction(5, 1, 0)], 4);
    let plan = talents::plan(&record, &forever_affliction(), 5).unwrap();
    let upcoming: Vec<(&str, u8)> = plan
        .upcoming
        .iter()
        .map(|p| (p.talent.as_str(), p.to_rank))
        .collect();
    assert_eq!(
        upcoming,
        vec![("Improved Bane of Agony", 2), ("Malediction", 4)]
    );
}

#[test]
fn a_tier_that_is_not_open_stops_the_plan_and_says_why() {
    // Nothing spent, and a build that starts in row 3.
    let record = warlock(vec![affliction(0, 0, 0)], 1);
    let build = TalentBuild {
        id: "test".into(),
        flavor: "forever".into(),
        class: "WARLOCK".into(),
        spec: "Affliction".into(),
        summary: String::new(),
        sources: vec![],
        notes: vec![],
        steps: vec![BuildStep {
            tree: "Affliction".into(),
            talent: "Pandemic".into(),
            rank: 1,
            tier: None,
        }],
    };
    let plan = talents::plan(&record, &build, 0).unwrap();
    assert!(plan.now.is_empty());
    assert_eq!(
        plan.blocked,
        Some(Blocked::TierLocked {
            tree: "Affliction".into(),
            talent: "Pandemic".into(),
            tier: 3,
            needed: 2 * POINTS_PER_TIER,
            spent: 0,
        })
    );
}

#[test]
fn an_arrow_needs_its_source_maxed() {
    let mut aff = affliction(5, 5, 5);
    // Twenty-five points so row 6 is open, but Siphon Life is untaken.
    aff.points_spent = Some(25);
    let record = warlock(vec![aff], 1);
    let build = TalentBuild {
        steps: vec![BuildStep {
            tree: "Affliction".into(),
            talent: "Shadow Mastery".into(),
            rank: 1,
            tier: None,
        }],
        ..forever_affliction()
    };
    let plan = talents::plan(&record, &build, 0).unwrap();
    assert_eq!(
        plan.blocked,
        Some(Blocked::NeedsPrerequisite {
            tree: "Affliction".into(),
            talent: "Shadow Mastery".into(),
            requires: "Siphon Life".into(),
        })
    );
}

#[test]
fn a_talent_the_tree_does_not_have_is_named_not_skipped() {
    let record = warlock(vec![affliction(5, 1, 4)], 1);
    let build = TalentBuild {
        steps: vec![BuildStep {
            tree: "Affliction".into(),
            talent: "Curse of Exhaustion".into(),
            rank: 1,
            tier: None,
        }],
        ..forever_affliction()
    };
    let plan = talents::plan(&record, &build, 0).unwrap();
    assert!(matches!(plan.blocked, Some(Blocked::UnknownTalent { .. })));
    assert!(!plan.finished);
}

#[test]
fn points_outside_the_build_are_reported() {
    let mut aff = affliction(5, 1, 0);
    aff.talents.as_mut().unwrap()[3].rank = Some(2); // Improved Drain Soul
    let record = warlock(vec![aff], 0);
    let plan = talents::plan(&record, &forever_affliction(), 0).unwrap();
    assert_eq!(plan.off_plan.len(), 1);
    assert_eq!(plan.off_plan[0].talent, "Improved Drain Soul");
    assert_eq!(plan.off_plan[0].build_rank, 0);
}

#[test]
fn a_build_never_spends_past_the_games_maximum() {
    // The Beast Mastery build asks for Intimidation 2; if the game says 1,
    // the game is right.
    let record = CharacterRecord {
        class: Some("HUNTER".into()),
        game_flavor: Some("forever".into()),
        talents: Some(Talents {
            trees: Some(vec![tree(
                "Beast Mastery",
                vec![talent("Intimidation", 1, 1, 0, 1)],
            )]),
            unspent_points: Some(3),
        }),
        ..Default::default()
    };
    let build = TalentBuild {
        id: "t".into(),
        flavor: "forever".into(),
        class: "HUNTER".into(),
        spec: "Beast Mastery".into(),
        summary: String::new(),
        sources: vec![],
        notes: vec![],
        steps: vec![BuildStep {
            tree: "Beast Mastery".into(),
            talent: "Intimidation".into(),
            rank: 2,
            tier: None,
        }],
    };
    let plan = talents::plan(&record, &build, 0).unwrap();
    assert_eq!(plan.now.len(), 1);
    assert_eq!(plan.now[0].to_rank, 1);
    assert!(plan.finished);
}

#[test]
fn totals_alone_are_not_enough_to_plan() {
    let record = warlock(
        vec![TalentTree {
            name: Some("Affliction".into()),
            points_spent: Some(6),
            talents: None,
        }],
        3,
    );
    assert_eq!(
        talents::plan(&record, &forever_affliction(), 5),
        Err(PlanError::NoTalentDetail)
    );
}

#[test]
fn the_spec_is_stated_inferred_or_honestly_unknown() {
    let record = warlock(vec![affliction(5, 1, 0), tree("Demonology", vec![])], 0);
    assert_eq!(
        talents::choose_spec(&record, Some("Demonology")),
        SpecChoice::Stated("Demonology".into())
    );
    assert_eq!(
        talents::choose_spec(&record, None),
        SpecChoice::Inferred("Affliction".into())
    );

    let fresh = warlock(vec![affliction(0, 0, 0), tree("Demonology", vec![])], 1);
    assert_eq!(talents::choose_spec(&fresh, None), SpecChoice::Unknown);
}

#[test]
fn builds_are_matched_by_client_class_and_spec() {
    let all = builds();
    let record = warlock(vec![affliction(5, 1, 0)], 0);
    assert_eq!(
        talents::find_build(&all, &record, "affliction").map(|b| b.id.as_str()),
        Some("forever-warlock-affliction")
    );
    let tbc = CharacterRecord {
        game_flavor: Some("tbc_classic".into()),
        ..record.clone()
    };
    assert_eq!(
        talents::find_build(&all, &tbc, "Affliction").map(|b| b.id.as_str()),
        Some("tbc-warlock-affliction")
    );
    assert!(talents::find_build(&all, &record, "Destruction").is_none());
}

/// Every shipped build must be spendable on its own terms: cumulative ranks
/// that only ever rise, and every step whose guide shows a tier reachable by
/// the points the build has already put in that tree. A build that fails this
/// would stop every character that follows it.
#[test]
fn every_shipped_build_is_spendable() {
    for build in builds() {
        assert!(!build.sources.is_empty(), "{} has no source", build.id);
        let mut ranks: BTreeMap<(String, String), u8> = BTreeMap::new();
        let mut spent: BTreeMap<String, u16> = BTreeMap::new();
        let mut total = 0u16;
        for step in &build.steps {
            let key = (step.tree.clone(), step.talent.clone());
            let before = ranks.get(&key).copied().unwrap_or(0);
            assert!(
                step.rank > before,
                "{}: {} does not rise",
                build.id,
                step.talent
            );
            if let Some(tier) = step.tier {
                let in_tree = spent.get(&step.tree).copied().unwrap_or(0);
                assert!(
                    in_tree >= u16::from(tier - 1) * POINTS_PER_TIER,
                    "{}: {} (row {tier}) reached with only {in_tree} points in {}",
                    build.id,
                    step.talent,
                    step.tree
                );
            }
            let added = u16::from(step.rank - before);
            *spent.entry(step.tree.clone()).or_default() += added;
            total += added;
            ranks.insert(key, step.rank);
        }
        let cap = if build.flavor == "tbc_classic" {
            61
        } else {
            51
        };
        assert!(total <= cap, "{} spends {total} points", build.id);
    }
}

#[test]
fn the_fixtures_detail_round_trips_through_the_parser() {
    for path in [
        "fixtures/collector-v2.forever.lua",
        "fixtures/collector-v2.tbc.lua",
    ] {
        let text = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + path,
        )
        .unwrap();
        let loaded = collector::load(&text).unwrap();
        let record = loaded.db.characters.values().next().unwrap();
        let frost = record
            .talents
            .as_ref()
            .unwrap()
            .trees
            .as_ref()
            .unwrap()
            .iter()
            .find(|t| t.name.as_deref() == Some("Frost"))
            .unwrap();
        let detail = frost.talents.as_ref().expect("per-talent detail parsed");
        let frostbolt = detail
            .iter()
            .find(|t| t.name.as_deref() == Some("Improved Frostbolt"))
            .unwrap();
        assert_eq!((frostbolt.tier, frostbolt.column), (Some(1), Some(2)));
        assert_eq!((frostbolt.rank, frostbolt.max_rank), (Some(5), Some(5)));
        let permafrost = detail
            .iter()
            .find(|t| t.name.as_deref() == Some("Permafrost"))
            .unwrap();
        assert_eq!(
            permafrost.requires,
            Some(TalentPosition { tier: 1, column: 2 })
        );
        let arcane = record.talents.as_ref().unwrap().trees.as_ref().unwrap()[0].clone();
        assert!(
            arcane.talents.is_none(),
            "{path}: an empty tree is not captured as empty"
        );
    }
}
