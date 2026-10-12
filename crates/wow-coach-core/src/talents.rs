//! Talent planning: where the next points go.
//!
//! Two inputs, kept apart on purpose. **The tree** comes from the game: the
//! collector records every talent's tier, column, rank, maximum and
//! prerequisite, for whatever class the character is. Nothing here hard-codes
//! a class's tree, which is why the planner works for every class on both
//! client families without a per-class table to go stale.
//!
//! **The build** is a recommendation: an ordered list of "bring this talent to
//! this rank" steps, written from a published guide and shipped as data in
//! `data/talent-builds/`. A build is the only part that has to be researched
//! per class and spec. When none exists, the planner says so by name rather
//! than inventing one.
//!
//! The planner walks the build against the character's real tree, one point at
//! a time, applying the game's two rules: a tier opens at five points per tier
//! already spent in that tree, and a talent with an arrow into it needs the
//! source talent maxed. It never skips a step to keep going — a build whose
//! next step is blocked means the character has spent off-plan, and the right
//! output is to say that, not to quietly pick something else.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::collector::{CharacterRecord, Talent, TalentTree};

/// Points per tier, the same on every client this tool reads.
pub const POINTS_PER_TIER: u16 = 5;

/// One recommended build for one class and spec on one client family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TalentBuild {
    /// Stable identifier, e.g. `forever-warlock-affliction-leveling`.
    pub id: String,
    /// The collector's `gameFlavor`: `forever` or `tbc_classic`.
    pub flavor: String,
    /// The collector's locale-independent class token, e.g. `WARLOCK`.
    pub class: String,
    /// The tree the build is named for, as the game names it.
    pub spec: String,
    /// One line for the player: what this build is for.
    pub summary: String,
    /// Where the order came from. A build without a source is an opinion.
    pub sources: Vec<String>,
    /// Caveats worth showing, such as steps not yet confirmed in game.
    #[serde(default)]
    pub notes: Vec<String>,
    pub steps: Vec<BuildStep>,
}

/// Bring `talent` in `tree` up to `rank`. Steps are cumulative targets, not
/// increments, so a character already past a step simply moves on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildStep {
    pub tree: String,
    pub talent: String,
    pub rank: u8,
    /// The tier the guide shows, when it shows one. Used only to check that a
    /// shipped build is spendable; the live tree's own tier always wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<u8>,
}

/// Every build shipped with the core.
pub fn builtin_builds() -> Vec<TalentBuild> {
    const FILES: &[&str] = &[
        include_str!("../data/talent-builds/forever-warlock-affliction.json"),
        include_str!("../data/talent-builds/forever-mage-frost.json"),
        include_str!("../data/talent-builds/forever-shaman-enhancement.json"),
        include_str!("../data/talent-builds/forever-hunter-beast-mastery.json"),
        include_str!("../data/talent-builds/tbc-warlock-affliction.json"),
        include_str!("../data/talent-builds/tbc-mage-frost.json"),
    ];
    FILES
        .iter()
        .map(|text| serde_json::from_str(text).expect("shipped talent build must parse"))
        .collect()
}

/// Which tree a character is being built in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecChoice {
    /// The player said so.
    Stated(String),
    /// Not stated; the tree holding the most points, when one clearly does.
    Inferred(String),
    /// Not stated and no tree leads, so there is nothing honest to infer.
    Unknown,
}

impl SpecChoice {
    pub fn tree(&self) -> Option<&str> {
        match self {
            Self::Stated(tree) | Self::Inferred(tree) => Some(tree),
            Self::Unknown => None,
        }
    }
}

/// The player's stated spec wins; otherwise the clear leader by points spent.
pub fn choose_spec(record: &CharacterRecord, stated: Option<&str>) -> SpecChoice {
    if let Some(tree) = stated {
        return SpecChoice::Stated(tree.to_string());
    }
    let Some(trees) = record.talents.as_ref().and_then(|t| t.trees.as_ref()) else {
        return SpecChoice::Unknown;
    };
    let mut ranked: Vec<(&str, u16)> = trees
        .iter()
        .filter_map(|tree| Some((tree.name.as_deref()?, tree.points_spent.unwrap_or(0))))
        .collect();
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    match ranked.as_slice() {
        [(name, top), rest @ ..] if *top > 0 && rest.iter().all(|(_, n)| n < top) => {
            SpecChoice::Inferred((*name).to_string())
        }
        _ => SpecChoice::Unknown,
    }
}

/// The shipped build for this character and spec, if one exists.
pub fn find_build<'a>(
    builds: &'a [TalentBuild],
    record: &CharacterRecord,
    spec: &str,
) -> Option<&'a TalentBuild> {
    let flavor = record.game_flavor.as_deref()?;
    let class = record.class.as_deref()?;
    builds.iter().find(|build| {
        build.flavor == flavor
            && build.class.eq_ignore_ascii_case(class)
            && build.spec.eq_ignore_ascii_case(spec)
    })
}

/// A run of consecutive points into one talent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedPoints {
    pub tree: String,
    pub talent: String,
    pub from_rank: u8,
    pub to_rank: u8,
    pub max_rank: u8,
    pub tier: u8,
    pub column: u8,
}

impl PlannedPoints {
    pub fn points(&self) -> u8 {
        self.to_rank - self.from_rank
    }
}

/// Why the plan stopped before the build ran out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Blocked {
    /// The build names a talent the game's tree does not have — a rename on
    /// this client, or a build written for a different one.
    UnknownTalent { tree: String, talent: String },
    /// The tier is not open yet and the build gives no earlier point to open it.
    TierLocked {
        tree: String,
        talent: String,
        tier: u8,
        needed: u16,
        spent: u16,
    },
    /// The arrow's source talent is not maxed.
    NeedsPrerequisite {
        tree: String,
        talent: String,
        requires: String,
    },
}

/// Points spent somewhere the build does not put them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OffPlan {
    pub tree: String,
    pub talent: String,
    pub rank: u8,
    /// What the build wants there in total: zero when the talent is not in
    /// the build at all.
    pub build_rank: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TalentPlan {
    pub build_id: String,
    pub unspent: u16,
    /// Where the unspent points go now, in order.
    pub now: Vec<PlannedPoints>,
    /// The points after those, for the levels ahead.
    pub upcoming: Vec<PlannedPoints>,
    /// Set when the walk could not continue.
    pub blocked: Option<Blocked>,
    /// True when every step of the build is already done.
    pub finished: bool,
    pub off_plan: Vec<OffPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// No talent capture at all.
    NoTalents,
    /// Totals only: the record predates per-talent capture, or the node API
    /// failed on this client.
    NoTalentDetail,
}

/// Walk `build` against the character's tree.
///
/// `lookahead` is how many points past the unspent ones to show as upcoming.
pub fn plan(
    record: &CharacterRecord,
    build: &TalentBuild,
    lookahead: u16,
) -> Result<TalentPlan, PlanError> {
    let talents = record.talents.as_ref().ok_or(PlanError::NoTalents)?;
    let trees = talents.trees.as_ref().ok_or(PlanError::NoTalents)?;
    if !trees
        .iter()
        .any(|tree| tree.talents.as_ref().is_some_and(|t| !t.is_empty()))
    {
        return Err(PlanError::NoTalentDetail);
    }
    let unspent = talents.unspent_points.unwrap_or(0);

    let mut state = State::new(trees);
    let mut steps_out: Vec<(String, String, u8, u8, u8, u8)> = Vec::new();
    let mut blocked = None;
    let budget = u32::from(unspent) + u32::from(lookahead);

    'steps: for step in &build.steps {
        let Some(slot) = state.find(&step.tree, &step.talent) else {
            blocked = Some(Blocked::UnknownTalent {
                tree: step.tree.clone(),
                talent: step.talent.clone(),
            });
            break;
        };
        while state.rank(slot) < step.rank {
            if steps_out.len() as u32 >= budget {
                break 'steps;
            }
            if let Some(reason) = state.why_not(slot) {
                blocked = Some(reason);
                break 'steps;
            }
            let (tier, column, max) = state.position(slot);
            if state.rank(slot) >= max {
                // The build asks for more than the talent holds; the game is
                // right and the step is done.
                break;
            }
            state.buy(slot);
            let (tree, name) = state.names(slot);
            steps_out.push((tree, name, state.rank(slot), max, tier, column));
        }
    }

    let finished = blocked.is_none()
        && build.steps.iter().all(|step| {
            state
                .find(&step.tree, &step.talent)
                .is_some_and(|slot| state.rank(slot) >= step.rank.min(state.position(slot).2))
        });

    let now_count = usize::from(unspent).min(steps_out.len());
    let now = group(&steps_out[..now_count]);
    let upcoming = group(&steps_out[now_count..]);

    Ok(TalentPlan {
        build_id: build.id.clone(),
        unspent,
        now,
        upcoming,
        blocked,
        finished,
        off_plan: off_plan(trees, build),
    })
}

fn group(points: &[(String, String, u8, u8, u8, u8)]) -> Vec<PlannedPoints> {
    let mut out: Vec<PlannedPoints> = Vec::new();
    for (tree, talent, rank_after, max, tier, column) in points {
        if let Some(last) = out.last_mut() {
            if &last.tree == tree && &last.talent == talent && last.to_rank + 1 == *rank_after {
                last.to_rank = *rank_after;
                continue;
            }
        }
        out.push(PlannedPoints {
            tree: tree.clone(),
            talent: talent.clone(),
            from_rank: rank_after - 1,
            to_rank: *rank_after,
            max_rank: *max,
            tier: *tier,
            column: *column,
        });
    }
    out
}

fn off_plan(trees: &[TalentTree], build: &TalentBuild) -> Vec<OffPlan> {
    let mut wanted: BTreeMap<(String, String), u8> = BTreeMap::new();
    for step in &build.steps {
        let key = (step.tree.to_lowercase(), step.talent.to_lowercase());
        let entry = wanted.entry(key).or_insert(0);
        *entry = (*entry).max(step.rank);
    }
    let mut out = Vec::new();
    for tree in trees {
        let tree_name = tree.name.clone().unwrap_or_default();
        for talent in tree.talents.iter().flatten() {
            let rank = talent.rank.unwrap_or(0);
            let Some(name) = talent.name.clone() else {
                continue;
            };
            let build_rank = wanted
                .get(&(tree_name.to_lowercase(), name.to_lowercase()))
                .copied()
                .unwrap_or(0);
            if rank > build_rank {
                out.push(OffPlan {
                    tree: tree_name.clone(),
                    talent: name,
                    rank,
                    build_rank,
                });
            }
        }
    }
    out
}

/// The simulated tree the planner spends into.
struct State<'a> {
    trees: &'a [TalentTree],
    ranks: Vec<Vec<u8>>,
    spent: Vec<u16>,
}

type Slot = (usize, usize);

impl<'a> State<'a> {
    fn new(trees: &'a [TalentTree]) -> Self {
        let ranks = trees
            .iter()
            .map(|tree| {
                tree.talents
                    .iter()
                    .flatten()
                    .map(|t| t.rank.unwrap_or(0))
                    .collect()
            })
            .collect();
        // The tree's own total, not a sum of talents: it is what the game
        // gates tiers on, and it stays right even if a talent was missed.
        let spent = trees
            .iter()
            .map(|tree| {
                let summed: u16 = tree
                    .talents
                    .iter()
                    .flatten()
                    .map(|t| u16::from(t.rank.unwrap_or(0)))
                    .sum();
                tree.points_spent.unwrap_or(summed).max(summed)
            })
            .collect();
        Self {
            trees,
            ranks,
            spent,
        }
    }

    fn talent(&self, (tree, index): Slot) -> &Talent {
        &self.trees[tree].talents.as_ref().expect("slot has talents")[index]
    }

    fn find(&self, tree: &str, talent: &str) -> Option<Slot> {
        let t = self.trees.iter().position(|candidate| {
            candidate
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(tree))
        })?;
        let i = self.trees[t]
            .talents
            .as_ref()?
            .iter()
            .position(|candidate| {
                candidate
                    .name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(talent))
            })?;
        Some((t, i))
    }

    fn rank(&self, (tree, index): Slot) -> u8 {
        self.ranks[tree][index]
    }

    fn position(&self, slot: Slot) -> (u8, u8, u8) {
        let talent = self.talent(slot);
        (
            talent.tier.unwrap_or(1),
            talent.column.unwrap_or(0),
            talent.max_rank.unwrap_or(u8::MAX),
        )
    }

    fn names(&self, slot: Slot) -> (String, String) {
        (
            self.trees[slot.0].name.clone().unwrap_or_default(),
            self.talent(slot).name.clone().unwrap_or_default(),
        )
    }

    fn why_not(&self, slot: Slot) -> Option<Blocked> {
        let talent = self.talent(slot);
        let (tree_name, talent_name) = self.names(slot);
        let tier = talent.tier.unwrap_or(1);
        let needed = u16::from(tier.saturating_sub(1)) * POINTS_PER_TIER;
        if self.spent[slot.0] < needed {
            return Some(Blocked::TierLocked {
                tree: tree_name,
                talent: talent_name,
                tier,
                needed,
                spent: self.spent[slot.0],
            });
        }
        if let Some(required) = talent.requires {
            let source = self.trees[slot.0]
                .talents
                .iter()
                .flatten()
                .enumerate()
                .find(|(_, t)| t.tier == Some(required.tier) && t.column == Some(required.column));
            if let Some((index, source)) = source {
                let max = source.max_rank.unwrap_or(0);
                if self.ranks[slot.0][index] < max {
                    return Some(Blocked::NeedsPrerequisite {
                        tree: tree_name,
                        talent: talent_name,
                        requires: source.name.clone().unwrap_or_default(),
                    });
                }
            }
        }
        None
    }

    fn buy(&mut self, (tree, index): Slot) {
        self.ranks[tree][index] += 1;
        self.spent[tree] += 1;
    }
}
