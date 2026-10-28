//! Browser bindings for the WoW Coach core.
//!
//! No rules live here. Everything in this file parses, reshapes or forwards,
//! because the architecture says frontends are presentation only and neither
//! re-implements a rule — and a binding layer is the easiest place for a rule
//! to accidentally grow a second home.
//!
//! See `README.md` for why this is a separate crate from the core, and for the
//! two shapes that cross the boundary.

use std::collections::BTreeMap;

use serde::Serialize;
use wasm_bindgen::prelude::*;
use wow_coach_core::auctionator::PriceDb;
use wow_coach_core::coaching::{self, Advice};
use wow_coach_core::collector::{self, CharacterRecord};
use wow_coach_core::economy;
use wow_coach_core::gear::{self, GearPlan, Standing as GearStanding};
use wow_coach_core::professions::{self, Difficulty, ProfessionPlan, SkillUp, Training};
use wow_coach_core::quests::{self, QuestPlan, QuestRef, Standing, ZoneCluster};
use wow_coach_core::roster::{self, Role, RosterConfig};

/// One player's data, parsed once and held.
///
/// The page constructs this with the contents of their SavedVariables file and
/// then asks it questions. Parsing on every call would be simpler and would
/// re-read the whole file to answer "what are my characters called".
#[wasm_bindgen]
#[derive(Debug)]
pub struct Session {
    characters: BTreeMap<String, CharacterRecord>,
    config: RosterConfig,
    notices: Vec<String>,
}

/// The work, in plain Rust.
///
/// Separate from the `#[wasm_bindgen]` block below so it can be tested on a
/// native target. A binding layer is exactly where a wrong field name or a
/// wrong nesting reaches the page unnoticed, so it does not get to be the one
/// untested thing in the project.
impl Session {
    pub fn open(saved_variables: &str) -> Result<Session, String> {
        let loaded = collector::load(saved_variables)
            .map_err(|error| format!("could not read that file: {error}"))?;
        Ok(Session {
            characters: loaded.db.characters,
            config: RosterConfig::default(),
            notices: loaded.notices,
        })
    }

    pub fn notices_json(&self) -> Result<String, String> {
        json(&self.notices)
    }

    pub fn characters_json(&self) -> Result<String, String> {
        let views: Vec<CharacterView> = roster::roster(&self.characters, &self.config)
            .iter()
            .map(CharacterView::from)
            .collect();
        json(&views)
    }

    pub fn apply_roles(&mut self, roles_json: &str) -> Result<(), String> {
        let roles: BTreeMap<String, String> = serde_json::from_str(roles_json)
            .map_err(|error| format!("roles must be a key to role map: {error}"))?;
        // Validated in full before anything is written, so a typo in the
        // fourth entry cannot leave the first three applied.
        let mut parsed = Vec::with_capacity(roles.len());
        for (key, label) in roles {
            if !self.characters.contains_key(&key) {
                return Err(format!("no character with the key {key}"));
            }
            let role = Role::parse(&label).ok_or_else(|| format!("{label} is not a role"))?;
            parsed.push((key, role));
        }
        for (key, role) in parsed {
            self.config.roles.insert(key, role);
        }
        Ok(())
    }

    pub fn recommendation_json(&self) -> Result<String, String> {
        let entries = roster::roster(&self.characters, &self.config);
        let advice = coaching::advise(&entries, true);
        json(&RecommendationView::from(&advice))
    }

    pub fn professions_json(&self) -> Result<String, String> {
        let entries = roster::roster(&self.characters, &self.config);
        // No price file in the browser — Auctionator import is not in v1. The
        // survey is still how holdings are gathered, and an unpriced reagent
        // is just as present in the bag as a priced one, so the empty price db
        // costs nothing here. `unpriced` is chained in for exactly that
        // reason: dropping it would silently hide materials the player has.
        let prices = PriceDb::default();
        let survey = economy::survey(&entries, &prices);
        let holdings: Vec<_> = survey
            .holdings
            .iter()
            .chain(&survey.unpriced)
            .cloned()
            .collect();
        let views: Vec<ProfessionView> = professions::plan(&entries, &holdings)
            .iter()
            .map(ProfessionView::from)
            .collect();
        json(&views)
    }

    pub fn gear_json(&self, key: &str) -> Result<String, String> {
        let record = self
            .characters
            .get(key)
            .ok_or_else(|| format!("no character with the key {key}"))?;
        let name = record.name.clone().unwrap_or_else(|| key.to_string());
        json(&GearPlanView::from(&gear::plan(record, &name)))
    }

    pub fn quests_json(&self, key: &str) -> Result<String, String> {
        let record = self
            .characters
            .get(key)
            .ok_or_else(|| format!("no character with the key {key}"))?;
        json(&QuestPlanView::from(&quests::plan(record, key)))
    }
}

#[wasm_bindgen]
impl Session {
    /// Parse a `WoWCoachCollector.lua` SavedVariables file.
    ///
    /// Fails loudly on anything outside the restricted subset the addon emits.
    /// A file this cannot read means the collector has a bug, and saying so is
    /// more useful than recovering something plausible.
    #[wasm_bindgen(constructor)]
    pub fn new(saved_variables: &str) -> Result<Session, JsError> {
        Session::open(saved_variables).map_err(|error| JsError::new(&error))
    }

    /// Anything the parse wanted to say that was not fatal — a newer schema, a
    /// record missing a field. Surfaced rather than swallowed.
    ///
    /// A method, deliberately, and not a `#[wasm_bindgen(getter)]`. It was a
    /// getter once: the page read `session.notices()`, JS saw a property, and
    /// the call threw `notices is not a function` — a blank screen after a
    /// file was dropped, which is the worst possible moment for one. The CI
    /// grep could not see it, because both sides spell it `notices` and only
    /// the attribute decides what JS gets. The other three accessors here are
    /// methods, this one can fail, and a getter that throws is a trap in any
    /// case. So: method, like its neighbours.
    pub fn notices(&self) -> Result<String, JsError> {
        self.notices_json().map_err(|error| JsError::new(&error))
    }

    /// Every character, with the role currently set for it.
    ///
    /// This is the first screen: the moment the tool proves it read the game
    /// correctly, by showing the player their own characters.
    pub fn characters(&self) -> Result<String, JsError> {
        self.characters_json().map_err(|error| JsError::new(&error))
    }

    /// Mark which characters are being played.
    ///
    /// Takes the whole set rather than one at a time, because the first-run
    /// question is "which of these are you actually playing" and the answer
    /// arrives as a set of tick boxes. Keys not named are left alone.
    pub fn set_roles(&mut self, roles_json: &str) -> Result<(), JsError> {
        self.apply_roles(roles_json)
            .map_err(|error| JsError::new(&error))
    }

    /// What to play next, and why.
    ///
    /// One answer, not a list — a coach gives one, and a list of five with
    /// reasons is a thing people skim and close. The rest of the ranking comes
    /// back underneath for anyone who wants it.
    pub fn recommendation(&self) -> Result<String, JsError> {
        self.recommendation_json()
            .map_err(|error| JsError::new(&error))
    }

    /// Every profession on every character: where the skill stands, whether a
    /// trainer can help yet, and what is still worth making.
    ///
    /// The whole roster in one call, not one character at a time. Materials
    /// are counted across bags and banks of every character, so a per-character
    /// call would have to survey the roster anyway and would invite the page to
    /// loop — which is how the same work gets done twenty-two times.
    pub fn professions(&self) -> Result<String, JsError> {
        self.professions_json()
            .map_err(|error| JsError::new(&error))
    }

    /// One character's gear: the slots sitting empty, and the step-change
    /// items worth going out of the way for.
    ///
    /// Per character, like quests: a wardrobe belongs to one character and a
    /// whole-roster view would be twenty-two lists nobody reads.
    pub fn gear(&self, key: &str) -> Result<String, JsError> {
        self.gear_json(key).map_err(|error| JsError::new(&error))
    }

    /// One character's quest log: what to hand in, where to go, and whether
    /// the log is too full to accept anything.
    ///
    /// Per character, unlike professions, because a quest log is genuinely
    /// local to one character and the whole-roster view would be noise.
    pub fn quests(&self, key: &str) -> Result<String, JsError> {
        self.quests_json(key).map_err(|error| JsError::new(&error))
    }
}

/// A character as the roster screen shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CharacterView {
    key: String,
    name: String,
    level: Option<u16>,
    /// Locale-independent class file name, e.g. `MAGE`. The page picks the
    /// colour and the display name; this is the identity.
    class: Option<String>,
    race: Option<String>,
    faction: Option<String>,
    realm: Option<String>,
    zone: Option<String>,
    /// `forever`, `tbc_classic`, `classic_era`. A roster can span clients.
    flavor: Option<String>,
    /// `unset`, `active`, `bank`, `parked` or `reserved`.
    role: &'static str,
    /// Unix epoch seconds of the capture, so the page can say how stale it is.
    captured_at: Option<i64>,
}

impl From<&roster::RosterEntry<'_>> for CharacterView {
    fn from(entry: &roster::RosterEntry<'_>) -> Self {
        let record = entry.record;
        CharacterView {
            key: entry.key.to_string(),
            name: entry.name().to_string(),
            level: record.level,
            class: record.class.clone(),
            race: record.race.clone(),
            faction: record.faction.clone(),
            realm: record.realm.clone(),
            zone: record.zone.clone(),
            flavor: record.game_flavor.clone(),
            role: entry.role.label(),
            captured_at: record.captured_at,
        }
    }
}

/// The one answer, plus what sits behind it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecommendationView {
    /// Absent when nothing is in the rotation yet — which is the first-run
    /// state, not an error, and the page should ask rather than apologise.
    play_next: Option<SuggestionView>,
    /// The remainder of the ranking, best first.
    then: Vec<SuggestionView>,
    /// Resting nowhere, so accruing nothing. Parking them is the fix.
    park: Vec<SuggestionView>,
    caveats: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SuggestionView {
    key: String,
    name: String,
    level: Option<u16>,
    /// The finished sentence, built in the core so every client says the same
    /// thing. The page displays it; it does not assemble it.
    headline: String,
}

impl From<&coaching::Suggestion> for SuggestionView {
    fn from(suggestion: &coaching::Suggestion) -> Self {
        SuggestionView {
            key: suggestion.key.clone(),
            name: suggestion.name.clone(),
            level: suggestion.level,
            headline: suggestion.headline(),
        }
    }
}

impl From<&Advice> for RecommendationView {
    fn from(advice: &Advice) -> Self {
        let mut play = advice.play.iter().map(SuggestionView::from);
        RecommendationView {
            play_next: play.next(),
            then: play.collect(),
            park: advice.park.iter().map(SuggestionView::from).collect(),
            caveats: advice.caveats.clone(),
        }
    }
}

/// One profession on one character, as the detail screen shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfessionView {
    /// The roster key. The page groups by this, not by the display name, so a
    /// roster spanning clients cannot merge two characters who share a name.
    key: String,
    character: String,
    profession: String,
    rank: Option<u16>,
    max_rank: Option<u16>,
    training: TrainingView,
    skill_ups: Vec<SkillUpView>,
    /// Whether a trade skill window has ever been scraped for this.
    ///
    /// Carried to the page rather than folded into an empty `skillUps`,
    /// because "nothing left to make" and "never looked" point a reader in
    /// opposite directions and this project has been caught conflating absent
    /// with empty more than once.
    recipes_known: bool,
    /// Known, and every recipe in it is grey. Reads as "nothing to do" and
    /// means "go and learn more recipes".
    out_of_useful_recipes: bool,
    notes: Vec<String>,
}

/// Why a trainer can or cannot help, flattened for JSON but never collapsed.
///
/// The five states fail in different directions: short on skill means keep
/// crafting, short on level means the profession is not the thing to work on
/// at all. A boolean here would merge the two pieces of advice that matter
/// most.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrainingView {
    /// `notYet`, `due`, `shortOnSkill`, `shortOnLevel` or `finished`.
    state: &'static str,
    /// The tier a trainer would teach, when there is one.
    tier: Option<&'static str>,
    cap: Option<u16>,
    /// The level or skill still needed, for the two short states.
    needs: Option<u16>,
    /// The finished sentence from the core, so every client says the same
    /// thing. Absent when there is nothing to say.
    sentence: Option<String>,
}

impl TrainingView {
    fn new(training: &Training, profession: &str) -> Self {
        let sentence = professions::describe(training, profession);
        let (state, tier, cap, needs) = match training {
            Training::NotYet => ("notYet", None, None, None),
            Training::Finished => ("finished", None, None, None),
            Training::Due { tier } => ("due", Some(tier.name), Some(tier.cap), None),
            Training::ShortOnSkill { tier, needs } => (
                "shortOnSkill",
                Some(tier.name),
                Some(tier.cap),
                Some(*needs),
            ),
            Training::ShortOnLevel { tier, needs } => (
                "shortOnLevel",
                Some(tier.name),
                Some(tier.cap),
                Some(*needs),
            ),
        };
        TrainingView {
            state,
            tier,
            cap,
            needs,
            sentence,
        }
    }
}

/// A recipe that can still raise the skill.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillUpView {
    recipe: String,
    /// `optimal`, `medium`, `easy`, `trivial` or `unknown` — the game's
    /// orange / yellow / green / grey, by its own names. `unknown` is not
    /// `trivial`: one was never captured, the other will never skill up.
    difficulty: &'static str,
    can_make: u32,
    /// Reagent name and how many short, for anything missing.
    short_of: Vec<ShortOfView>,
    needs_a_bank_trip: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortOfView {
    reagent: String,
    short_by: u32,
}

impl From<&SkillUp> for SkillUpView {
    fn from(entry: &SkillUp) -> Self {
        SkillUpView {
            recipe: entry.recipe.clone(),
            difficulty: match entry.difficulty {
                Difficulty::Optimal => "optimal",
                Difficulty::Medium => "medium",
                Difficulty::Easy => "easy",
                Difficulty::Trivial => "trivial",
                Difficulty::Unknown => "unknown",
            },
            can_make: entry.can_make,
            short_of: entry
                .short_of
                .iter()
                .map(|(reagent, short_by)| ShortOfView {
                    reagent: reagent.clone(),
                    short_by: *short_by,
                })
                .collect(),
            needs_a_bank_trip: entry.needs_a_bank_trip,
        }
    }
}

impl From<&ProfessionPlan> for ProfessionView {
    fn from(plan: &ProfessionPlan) -> Self {
        ProfessionView {
            key: plan.key.clone(),
            character: plan.character.clone(),
            profession: plan.profession.clone(),
            rank: plan.rank,
            max_rank: plan.max_rank,
            training: TrainingView::new(&plan.training, &plan.profession),
            skill_ups: plan.skill_ups.iter().map(SkillUpView::from).collect(),
            recipes_known: plan.has_recipe_cache,
            out_of_useful_recipes: plan.out_of_useful_recipes(),
            notes: plan.notes.clone(),
        }
    }
}

/// One character's quest log, as the detail screen shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestPlanView {
    character: String,
    level: Option<u16>,
    /// Ready to hand in, wherever they are. First, always: they cost nothing
    /// but the walk and each one frees a slot.
    turn_ins: Vec<QuestRefView>,
    /// Zones, the one that clears the most first.
    clusters: Vec<ZoneClusterView>,
    /// The zone worth going to, when any zone clears anything. Picked by the
    /// core, not by the page re-deriving it from the list.
    best_zone: Option<String>,
    log: LogView,
    /// Only populated when the log is tight — a quest you will not do is
    /// harmless until it costs a slot.
    drop_candidates: Vec<QuestRefView>,
    caveats: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogView {
    used: usize,
    capacity: usize,
    free: usize,
    is_tight: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ZoneClusterView {
    zone: String,
    complete: Vec<QuestRefView>,
    ready: Vec<QuestRefView>,
    too_high: Vec<QuestRefView>,
    trivial: Vec<QuestRefView>,
    /// How much a visit could actually clear: complete plus ready. The reason
    /// to go, and the key the list is sorted by.
    worth_a_trip: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestRefView {
    /// The client's quest id, for linking out to a quest database. Absent when
    /// the collector did not capture one, and the page must then render the
    /// title as plain text rather than a link to `quest=undefined`.
    quest_id: Option<u32>,
    title: String,
    level: Option<u16>,
    zone: Option<String>,
    /// `ready`, `tooHigh`, `trivial` or `unknown`.
    standing: &'static str,
    /// How many levels above or below, for the two that have a distance.
    by: Option<u16>,
}

impl From<&QuestRef> for QuestRefView {
    fn from(quest: &QuestRef) -> Self {
        let (standing, by) = match quest.standing {
            Standing::Ready => ("ready", None),
            Standing::Unknown => ("unknown", None),
            Standing::TooHigh { by } => ("tooHigh", Some(by)),
            Standing::Trivial { by } => ("trivial", Some(by)),
        };
        QuestRefView {
            quest_id: quest.quest_id,
            title: quest.title.clone(),
            level: quest.level,
            zone: quest.zone.clone(),
            standing,
            by,
        }
    }
}

impl From<&ZoneCluster> for ZoneClusterView {
    fn from(cluster: &ZoneCluster) -> Self {
        ZoneClusterView {
            zone: cluster.zone.clone(),
            complete: cluster.complete.iter().map(QuestRefView::from).collect(),
            ready: cluster.ready.iter().map(QuestRefView::from).collect(),
            too_high: cluster.too_high.iter().map(QuestRefView::from).collect(),
            trivial: cluster.trivial.iter().map(QuestRefView::from).collect(),
            worth_a_trip: cluster.worth_a_trip(),
        }
    }
}

impl From<&QuestPlan> for QuestPlanView {
    fn from(plan: &QuestPlan) -> Self {
        QuestPlanView {
            character: plan.character.clone(),
            level: plan.level,
            turn_ins: plan.turn_ins.iter().map(QuestRefView::from).collect(),
            clusters: plan.clusters.iter().map(ZoneClusterView::from).collect(),
            best_zone: plan.best_zone().map(|cluster| cluster.zone.clone()),
            log: LogView {
                used: plan.log.used,
                capacity: plan.log.capacity,
                free: plan.log.free(),
                is_tight: plan.log.is_tight(),
            },
            drop_candidates: plan
                .drop_candidates
                .iter()
                .map(QuestRefView::from)
                .collect(),
            caveats: plan.caveats.clone(),
        }
    }
}

/// One character's gear, as the detail screen shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GearPlanView {
    character: String,
    level: Option<u16>,
    class: Option<String>,
    empty_slots: Vec<EmptySlotView>,
    milestones: Vec<MilestoneView>,
    /// False when no equipment has ever been captured. Everything else here is
    /// then silence, and the page must say so rather than render a clean
    /// screen that reads as "fully geared".
    equipment_known: bool,
    caveats: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmptySlotView {
    slot: u8,
    name: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MilestoneView {
    what: &'static str,
    /// The ladder this rung belongs to, so the page can group without
    /// matching on prose.
    line: &'static str,
    why: &'static str,
    how: &'static str,
    from_level: u16,
    /// `met`, `due`, `notYet` or `unknowable`.
    state: &'static str,
    /// The level it becomes relevant, for `notYet`.
    at_level: Option<u16>,
    /// Why it cannot be decided, for `unknowable`.
    because: Option<&'static str>,
    /// Where the claim came from. Carried to the screen rather than kept in
    /// the table, because a reader weighing advice needs to know who said it.
    source: &'static str,
    source_read_on: &'static str,
    /// False marks a claim carried over from Classic and never checked against
    /// a Forever source. The page shows it; dropping it here would launder an
    /// unverified claim into a confident one.
    checked_for_forever: bool,
}

impl From<&GearPlan> for GearPlanView {
    fn from(plan: &GearPlan) -> Self {
        GearPlanView {
            character: plan.character.clone(),
            level: plan.level,
            class: plan.class.clone(),
            empty_slots: plan
                .empty_slots
                .iter()
                .map(|slot| EmptySlotView {
                    slot: slot.slot,
                    name: slot.name,
                })
                .collect(),
            milestones: plan
                .milestones
                .iter()
                .map(|milestone| {
                    let (state, at_level, because) = match &milestone.standing {
                        GearStanding::Met => ("met", None, None),
                        GearStanding::Due => ("due", None, None),
                        GearStanding::NotYet { at_level } => ("notYet", Some(*at_level), None),
                        GearStanding::Unknowable { because } => {
                            ("unknowable", None, Some(*because))
                        }
                    };
                    MilestoneView {
                        what: milestone.what,
                        line: milestone.line,
                        why: milestone.why,
                        how: milestone.how,
                        from_level: milestone.from_level,
                        state,
                        at_level,
                        because,
                        source: milestone.source,
                        source_read_on: milestone.source_read_on,
                        checked_for_forever: milestone.checked_for_forever,
                    }
                })
                .collect(),
            equipment_known: plan.equipment_known,
            caveats: plan.caveats.clone(),
        }
    }
}

fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}
