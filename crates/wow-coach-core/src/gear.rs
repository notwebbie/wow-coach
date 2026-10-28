//! What a character is wearing, what is missing, and when to go and get it.
//!
//! Two layers, deliberately kept apart because they are made of different
//! stuff and age at different rates.
//!
//! **Gaps** are client truth. An empty slot is an empty slot; nothing here is
//! anybody's opinion, and nothing goes stale. This layer works on any client,
//! for any class, with no curated data at all.
//!
//! **Milestones** are curated outside knowledge: someone decided that a wand
//! matters at level five. That is a different kind of claim from everything
//! else in this core, which is either measured from the client or derived by
//! rule from those measurements. Every milestone therefore carries where it
//! came from, and [`MILESTONES`] is explicitly a seed rather than a finished
//! table — see `docs/SOURCES.md`.
//!
//! The split matters because the first layer keeps working when the second is
//! wrong. A player whose class has no milestones yet still gets told about the
//! three empty armour slots they have been running around in.

use crate::collector::{CharacterRecord, EquippedItem};

/// Equipment slot numbers, as the client indexes them.
///
/// Stored rather than inferred: the collector captures slot numbers without
/// interpreting them, and this is the one place that assigns them meaning.
pub mod slot {
    pub const HEAD: u8 = 1;
    pub const NECK: u8 = 2;
    pub const SHOULDER: u8 = 3;
    pub const SHIRT: u8 = 4;
    pub const CHEST: u8 = 5;
    pub const WAIST: u8 = 6;
    pub const LEGS: u8 = 7;
    pub const FEET: u8 = 8;
    pub const WRIST: u8 = 9;
    pub const HANDS: u8 = 10;
    pub const FINGER_1: u8 = 11;
    pub const FINGER_2: u8 = 12;
    pub const TRINKET_1: u8 = 13;
    pub const TRINKET_2: u8 = 14;
    pub const BACK: u8 = 15;
    pub const MAIN_HAND: u8 = 16;
    pub const OFF_HAND: u8 = 17;
    pub const RANGED: u8 = 18;
    pub const TABARD: u8 = 19;
}

/// The slots worth telling a player about when empty, with their names.
///
/// Shirt and tabard are left out on purpose: both are cosmetic, both are
/// empty on most levelling characters, and listing them would bury the three
/// gaps that cost real stats under two that cost none. Ranged is left out
/// here because whether it matters depends on the class, which the milestones
/// answer and a flat list cannot.
const REPORTABLE: &[(u8, &str)] = &[
    (slot::HEAD, "Head"),
    (slot::NECK, "Neck"),
    (slot::SHOULDER, "Shoulder"),
    (slot::CHEST, "Chest"),
    (slot::WAIST, "Waist"),
    (slot::LEGS, "Legs"),
    (slot::FEET, "Feet"),
    (slot::WRIST, "Wrist"),
    (slot::HANDS, "Hands"),
    (slot::FINGER_1, "Ring 1"),
    (slot::FINGER_2, "Ring 2"),
    (slot::TRINKET_1, "Trinket 1"),
    (slot::TRINKET_2, "Trinket 2"),
    (slot::BACK, "Back"),
    (slot::MAIN_HAND, "Main hand"),
    (slot::OFF_HAND, "Off hand"),
];

pub fn slot_name(slot: u8) -> Option<&'static str> {
    REPORTABLE
        .iter()
        .find(|(number, _)| *number == slot)
        .map(|(_, name)| *name)
        .or(match slot {
            slot::SHIRT => Some("Shirt"),
            slot::RANGED => Some("Ranged"),
            slot::TABARD => Some("Tabard"),
            _ => None,
        })
}

/// What a milestone asks the character to be carrying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// An equipped item the client calls this subtype — `Wands`, `Bows`.
    ///
    /// Matched on the item's own type rather than on which slot it sits in,
    /// because a wand's slot is a client detail and its being a wand is not.
    SubType(&'static str),
    /// The same, but only counting items at or above an item level.
    ///
    /// This is what makes an upgrade rung checkable. Without it "a better
    /// wand" is satisfied by the starter wand the rung exists to replace,
    /// because both are `Wands` and nothing else distinguishes them. An item
    /// whose level the client never cached cannot satisfy this — see
    /// `type_is_known`, which the undecidable path already handles.
    SubTypeAtLeast {
        sub_type: &'static str,
        item_level: u16,
    },
    /// Any one of several subtypes. Hunters are happy with a bow, a gun or a
    /// crossbow, and recommending one specifically would be inventing advice.
    AnySubType(&'static [&'static str]),
}

/// The slots a weapon can occupy.
///
/// Used to decide whether an item the client never described could possibly be
/// the thing being asked about. Weaker than naming the slot a wand lives in —
/// which is the client's business and changes between them — and strong enough
/// to rule out the rings, trinkets and chest pieces that obviously cannot be
/// a wand whatever else they are.
const WEAPON_SLOTS: &[u8] = &[slot::MAIN_HAND, slot::OFF_HAND, slot::RANGED];

impl Requirement {
    fn satisfied_by(&self, item: &EquippedItem) -> bool {
        match self {
            Self::SubType(want) => item.is_sub_type(want),
            Self::SubTypeAtLeast {
                sub_type,
                item_level,
            } => {
                item.is_sub_type(sub_type) && item.item_level.is_some_and(|got| got >= *item_level)
            }
            Self::AnySubType(wants) => wants.iter().any(|want| item.is_sub_type(want)),
        }
    }

    /// Whether an unidentified item in this slot could be the thing asked for,
    /// and so ought to block a conclusion either way.
    ///
    /// Every requirement in this table is a weapon subtype. The first version
    /// of this check had no slot filter at all: any unidentified item anywhere
    /// made every milestone unanswerable, and the committed fixture — a level
    /// 34 mage wearing two trinkets the client had not cached — came back as
    /// "cannot tell whether you have a wand" when the ranged slot was plainly
    /// empty. A check that covers more than it should is as wrong as one that
    /// covers too little, and harder to notice, because the output still looks
    /// careful.
    fn could_be_satisfied_by_slot(&self, slot: u8) -> bool {
        WEAPON_SLOTS.contains(&slot)
    }
}

/// Where a milestone's claim came from. Carried in the data rather than in a
/// comment so a reader can weigh it without leaving the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provenance {
    pub source: &'static str,
    /// ISO date the claim was read.
    pub read_on: &'static str,
    /// False when the claim is carried over from Classic rather than checked
    /// against a Forever source. Shown to the reader, never hidden.
    pub checked_for_forever: bool,
}

/// How the roster could make a milestone item, when it is craftable.
///
/// The recipe is named rather than identified by item id because the collector
/// captures a recipe's name and not what it produces. That makes the match
/// locale-dependent: an English client says "Greater Magic Wand" and a German
/// one does not. Known, accepted for now, and the reason a miss here is
/// reported as "nobody can make it" rather than silently dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CraftRoute {
    pub recipe: &'static str,
    pub profession: &'static str,
    /// Profession skill needed before a trainer will teach it, and before it
    /// can be made.
    pub needs_skill: u16,
}

/// One piece of gear that is a step change rather than an upgrade.
///
/// The bar for being in this table: a character who does not have it is
/// playing a meaningfully worse game, not a marginally worse one. "A slightly
/// better chest piece" never qualifies; "no wand at all" does.
#[derive(Debug, Clone, Copy)]
pub struct Milestone {
    pub what: &'static str,
    /// Rungs on one ladder share a line.
    ///
    /// Without this, a level 20 caster carrying nothing was told to get a
    /// starter wand *and* a better one — two instructions for one empty slot,
    /// the lower of which is obsolete the moment the higher applies. Only the
    /// most relevant rung in a line is ever reported.
    pub line: &'static str,
    /// Why it matters, in one sentence, for the player rather than the coder.
    pub why: &'static str,
    /// The level from which this is worth acting on.
    pub from_level: u16,
    /// Class file names it applies to, e.g. `MAGE`. Empty means every class.
    pub classes: &'static [&'static str],
    pub requirement: Requirement,
    /// Where to actually get one.
    pub how: &'static str,
    /// Set when somebody on the roster could make this rather than buy it.
    pub craft: Option<CraftRoute>,
    pub provenance: Provenance,
}

impl Milestone {
    fn applies_to(&self, class: Option<&str>) -> bool {
        if self.classes.is_empty() {
            return true;
        }
        class.is_some_and(|class| {
            self.classes
                .iter()
                .any(|want| want.eq_ignore_ascii_case(class))
        })
    }
}

const WAND_CLASSES: &[&str] = &["MAGE", "PRIEST", "WARLOCK"];
const HUNTER: &[&str] = &["HUNTER"];
const RANGED_WEAPONS: &[&str] = &["Bows", "Guns", "Crossbows"];

const TAVERN_WARLOCK: Provenance = Provenance {
    source: "Warcraft Tavern, Forever warlock levelling guide",
    read_on: "2026-10-28",
    checked_for_forever: true,
};

/// The milestone table.
///
/// **This is a seed, not a finished table.** It holds the handful of
/// step-changes that are well sourced for Forever today. Classes with no row
/// here are not classes with nothing to get; they are classes nobody has
/// sourced yet, and [`plan`] says so rather than reporting them as fine.
pub const MILESTONES: &[Milestone] = &[
    Milestone {
        what: "A wand",
        line: "wand",
        why: "Wanding out-damages casting at low levels and costs no mana, so a \
              caster without one is slower between every drink.",
        from_level: 5,
        classes: WAND_CLASSES,
        requirement: Requirement::SubType("Wands"),
        how: "Lesser Magic Wand is an Enchanting craft. Buy one from an enchanter or \
              the auction house if nobody on your side can make it.",
        craft: Some(CraftRoute {
            recipe: "Lesser Magic Wand",
            profession: "Enchanting",
            needs_skill: 15,
        }),
        provenance: TAVERN_WARLOCK,
    },
    Milestone {
        what: "A better wand",
        line: "wand",
        why: "The starter wand is outgrown by the teens, and wand damage is the \
              stat that matters most before 30.",
        from_level: 13,
        classes: WAND_CLASSES,
        // Greater Magic Wand is item level 23, so that is the bar a wand has
        // to clear to count as the upgrade rather than the thing being
        // upgraded from. Any other wand at 23 or above satisfies it equally.
        requirement: Requirement::SubTypeAtLeast {
            sub_type: "Wands",
            item_level: 23,
        },
        how: "Greater Magic Wand is the Enchanting craft for this bracket. Gravestone \
              Scepter, a Blackfathom Deeps quest reward, is the strongest before 30.",
        craft: Some(CraftRoute {
            recipe: "Greater Magic Wand",
            profession: "Enchanting",
            needs_skill: 70,
        }),
        provenance: TAVERN_WARLOCK,
    },
    Milestone {
        what: "A ranged weapon",
        line: "ranged",
        why: "A hunter opens every fight at range, and without a bow or gun there \
              is nothing to open with.",
        from_level: 1,
        classes: HUNTER,
        requirement: Requirement::AnySubType(RANGED_WEAPONS),
        how: "Hunters start with one. If the slot is empty it was sold or outgrown \
              — any vendor ranged weapon beats none.",
        craft: None,
        provenance: Provenance {
            source: "Carried over from Classic; not checked against a Forever source",
            read_on: "2026-10-28",
            checked_for_forever: false,
        },
    },
];

/// Whether a milestone is met, and if not, whether it can be answered at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Carrying it.
    Met,
    /// Old enough to want it, and not carrying it.
    Due,
    /// Not yet old enough.
    NotYet { at_level: u16 },
    /// Something is in the slot but the client never told us what it is, so
    /// whether it satisfies this cannot be decided. Reported rather than
    /// guessed either way.
    Unknowable { because: &'static str },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MilestoneStatus {
    pub what: &'static str,
    /// The ladder this rung belongs to, so a caller can group or look one up
    /// without matching on prose.
    pub line: &'static str,
    pub why: &'static str,
    pub how: &'static str,
    pub from_level: u16,
    pub standing: Standing,
    pub source: &'static str,
    pub source_read_on: &'static str,
    pub checked_for_forever: bool,
}

impl MilestoneStatus {
    pub fn is_actionable(&self) -> bool {
        matches!(self.standing, Standing::Due)
    }
}

/// An empty slot worth mentioning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptySlot {
    pub slot: u8,
    pub name: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GearPlan {
    pub character: String,
    pub level: Option<u16>,
    pub class: Option<String>,
    /// Empty when the equipment could be read and nothing is missing. Always
    /// empty when it could not be read — check `equipment_known` first.
    pub empty_slots: Vec<EmptySlot>,
    pub milestones: Vec<MilestoneStatus>,
    /// False when no equipment capture has run for this character. Everything
    /// else here is then silence rather than a clean bill of health.
    pub equipment_known: bool,
    pub caveats: Vec<String>,
}

impl GearPlan {
    /// The milestones worth acting on now.
    pub fn due(&self) -> impl Iterator<Item = &MilestoneStatus> {
        self.milestones.iter().filter(|m| m.is_actionable())
    }
}

/// Compute every applicable rung's standing.
fn rungs(record: &CharacterRecord, kit: &crate::collector::Equipment) -> Vec<MilestoneStatus> {
    let level = record.level;
    let class = record.class.as_deref();
    let mut statuses = Vec::new();

    for milestone in MILESTONES {
        if !milestone.applies_to(class) {
            continue;
        }

        let met = kit
            .worn()
            .iter()
            .any(|item| milestone.requirement.satisfied_by(item));

        let standing = if met {
            Standing::Met
        } else if level.is_some_and(|level| level < milestone.from_level) {
            Standing::NotYet {
                at_level: milestone.from_level,
            }
        } else if kit.worn().iter().any(|item| {
            !item.type_is_known()
                && item
                    .slot
                    .is_some_and(|slot| milestone.requirement.could_be_satisfied_by_slot(slot))
        }) {
            Standing::Unknowable {
                because: "a weapon is equipped that the game client had not loaded a \
                          description for, so it cannot be identified",
            }
        } else {
            Standing::Due
        };

        statuses.push(MilestoneStatus {
            what: milestone.what,
            line: milestone.line,
            why: milestone.why,
            how: milestone.how,
            from_level: milestone.from_level,
            standing,
            source: milestone.provenance.source,
            source_read_on: milestone.provenance.read_on,
            checked_for_forever: milestone.provenance.checked_for_forever,
        });
    }

    statuses
}

/// One rung per ladder. Which one depends on where the character is: something
/// to do now beats something to do later, and a rung already satisfied is only
/// worth showing when nothing above it is pending.
fn narrow(all: Vec<MilestoneStatus>) -> Vec<MilestoneStatus> {
    let mut lines: Vec<&'static str> = all.iter().map(|status| status.line).collect();
    lines.dedup();

    let mut kept = Vec::new();
    for line in lines {
        let rungs: Vec<&MilestoneStatus> =
            all.iter().filter(|status| status.line == line).collect();

        let chosen = rungs
            .iter()
            .filter(|rung| matches!(rung.standing, Standing::Due))
            .max_by_key(|rung| rung.from_level)
            .or_else(|| {
                rungs
                    .iter()
                    .filter(|rung| matches!(rung.standing, Standing::NotYet { .. }))
                    .min_by_key(|rung| rung.from_level)
            })
            .or_else(|| rungs.iter().max_by_key(|rung| rung.from_level));

        if let Some(rung) = chosen {
            kept.push((*rung).clone());
        }
    }
    kept
}

/// Every rung of every ladder that applies, unfiltered.
///
/// [`plan`] narrows this to one rung per ladder, which is what a screen wants.
/// Forward planning wants the rest: a caster with no wand at 11 should both
/// buy one now *and* know the upgrade lands at 13, and a single-rung view can
/// only say one of those. Returns an empty list when equipment was never
/// captured, for the same reason `plan` does.
pub fn statuses(record: &CharacterRecord) -> Vec<MilestoneStatus> {
    let Some(kit) = record.equipment.as_ref().filter(|kit| kit.is_usable()) else {
        return Vec::new();
    };
    rungs(record, kit)
}

/// Plan one character's gear.
pub fn plan(record: &CharacterRecord, name: &str) -> GearPlan {
    let level = record.level;
    let class = record.class.clone();
    let mut caveats = Vec::new();

    let equipment = record.equipment.as_ref().filter(|kit| kit.is_usable());
    let Some(kit) = equipment else {
        caveats.push(
            "No equipment has been captured for this character yet. Log in and out once \
             with the collector enabled and what is missing will show here."
                .to_string(),
        );
        return GearPlan {
            character: name.to_string(),
            level,
            class,
            empty_slots: Vec::new(),
            milestones: Vec::new(),
            equipment_known: false,
            caveats,
        };
    };

    // A two-handed weapon fills the off hand by occupying it, so reporting an
    // empty off hand beside a staff would be telling a mage to go and find a
    // shield they cannot use.
    let two_handed = kit
        .in_slot(slot::MAIN_HAND)
        .is_some_and(|item| item.is_equip_loc("INVTYPE_2HWEAPON"));

    let empty_slots = REPORTABLE
        .iter()
        .filter(|(number, _)| !(two_handed && *number == slot::OFF_HAND))
        .filter(|(number, _)| kit.in_slot(*number).is_none())
        .map(|(number, name)| EmptySlot {
            slot: *number,
            name,
        })
        .collect();

    let milestones = narrow(rungs(record, kit));

    // Saying nothing about a class nobody has researched would read as saying
    // there is nothing to get, which is the absent-is-not-empty trap wearing a
    // different hat.
    if milestones.is_empty() {
        let who = class.as_deref().unwrap_or("this class");
        caveats.push(format!(
            "No gear milestones have been recorded for {who} yet, so the list above is \
             silence rather than a clean bill of health. Empty slots are still reported."
        ));
    }

    GearPlan {
        character: name.to_string(),
        level,
        class,
        empty_slots,
        milestones,
        equipment_known: true,
        caveats,
    }
}
