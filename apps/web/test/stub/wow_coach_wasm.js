// A plain-JS stand-in for the wasm-pack output, used only to drive the page in
// a headless browser. It mirrors the shape of crates/wow-coach-wasm: a default
// init(), a Session constructor, and the accessors the page calls.
//
// The data is deliberately awkward in the places the page is most likely to get
// wrong: a profession whose recipe window has never been opened, a profession
// whose recipes are all grey, a reagent that falls short, a zone that holds
// only grey quests, and a full quest log.
export default async function init() { return {}; }

const GRAVEHEXX = "7:forever|16:Classic Beta PvE|9:Gravehexx";
const VARDRIL = "7:forever|16:Classic Beta PvE|7:Vardril";

export class Session {
  constructor(text) {
    if (!text.includes("WoWCoachCollectorDB")) {
      throw new Error("not a WoWCoachCollector SavedVariables file");
    }
    this.roles = {};
  }
  notices() { return JSON.stringify([]); }
  characters() {
    return JSON.stringify([
      { key: GRAVEHEXX, name: "Gravehexx", level: 14, class: "WARLOCK",
        race: "Human", faction: "Alliance", realm: "Spineshatter", zone: "Elwynn Forest",
        flavor: "forever", role: "unset", capturedAt: 1760000000 },
      { key: VARDRIL, name: "Vardril", level: 8, class: "MAGE",
        race: "Gnome", faction: "Alliance", realm: "Spineshatter", zone: "Dun Morogh",
        flavor: "forever", role: "unset", capturedAt: 1760000000 }
    ]);
  }
  set_roles(json) { this.roles = JSON.parse(json); }
  recommendation() {
    return JSON.stringify({
      playNext: { key: GRAVEHEXX, name: "Gravehexx", level: 14,
                  headline: "Play Gravehexx next — fully rested at 14." },
      then: [{ key: VARDRIL, name: "Vardril", level: 8,
               headline: "Vardril is half rested at 8." }],
      park: [],
      caveats: ["Rested figures are approximate on this client."]
    });
  }
  professions() {
    return JSON.stringify([
      { key: GRAVEHEXX, character: "Gravehexx", profession: "Tailoring",
        rank: 175, maxRank: 225,
        training: { state: "notYet", tier: null, cap: null, needs: null, sentence: null },
        skillUps: [
          { recipe: "Woolen Cape", difficulty: "optimal", canMake: 3,
            shortOf: [], needsABankTrip: false },
          { recipe: "Reinforced Woolen Shoulders", difficulty: "medium", canMake: 0,
            shortOf: [{ reagent: "Wool Cloth", shortBy: 4 }], needsABankTrip: false },
          { recipe: "Gray Woolen Robe", difficulty: "easy", canMake: 1,
            shortOf: [], needsABankTrip: true }
        ],
        recipesKnown: true, outOfUsefulRecipes: false, notes: [] },
      // Window never opened: the page must not call this "nothing to make".
      { key: GRAVEHEXX, character: "Gravehexx", profession: "Enchanting",
        rank: 12, maxRank: 75,
        training: { state: "notYet", tier: null, cap: null, needs: null, sentence: null },
        skillUps: [], recipesKnown: false, outOfUsefulRecipes: false, notes: [] },
      // At the cap and old enough: a trainer is the answer.
      { key: GRAVEHEXX, character: "Gravehexx", profession: "First Aid",
        rank: 75, maxRank: 75,
        training: { state: "due", tier: "Journeyman", cap: 150, needs: null,
                    sentence: "First Aid is capped at 75. A trainer will take it to 150." },
        skillUps: [], recipesKnown: true, outOfUsefulRecipes: true, notes: [] },
      { key: VARDRIL, character: "Vardril", profession: "Mining",
        rank: 40, maxRank: 75,
        training: { state: "shortOnLevel", tier: "Journeyman", cap: 150, needs: 10,
                    sentence: "Mining is capped at 75 and the next tier needs level 10." },
        skillUps: [], recipesKnown: false, outOfUsefulRecipes: false, notes: [] }
    ]);
  }
  gear(key) {
    if (key === VARDRIL) {
      // Never captured: the page must say so rather than render a clean screen.
      return JSON.stringify({
        character: "Vardril", level: 8, class: "MAGE",
        emptySlots: [], milestones: [], equipmentKnown: false,
        caveats: ["No equipment has been captured for this character yet. Log in and out once with the collector enabled and what is missing will show here."]
      });
    }
    return JSON.stringify({
      character: "Gravehexx", level: 14, class: "WARLOCK",
      emptySlots: [{ slot: 3, name: "Shoulder" }, { slot: 7, name: "Legs" }, { slot: 15, name: "Back" }],
      milestones: [
        { what: "A wand", why: "Wanding out-damages casting at low levels and costs no mana.",
          how: "Lesser Magic Wand is an Enchanting craft usable from level 5.",
          fromLevel: 5, state: "due", atLevel: null, because: null,
          source: "Warcraft Tavern, Forever warlock levelling guide",
          sourceReadOn: "2026-10-28", checkedForForever: true },
        { what: "A ranged weapon", why: "Nothing to open a fight with.",
          how: "Any vendor ranged weapon beats none.",
          fromLevel: 1, state: "unknowable",
          because: "a weapon is equipped that the game client had not loaded a description for, so it cannot be identified",
          atLevel: null,
          source: "Carried over from Classic; not checked against a Forever source",
          sourceReadOn: "2026-10-28", checkedForForever: false }
      ],
      equipmentKnown: true, caveats: []
    });
  }
  quests(key) {
    if (key === VARDRIL) {
      // Nothing worth doing anywhere, and a log with room. The page must not
      // invent a destination.
      return JSON.stringify({
        character: "Vardril", level: 8, turnIns: [],
        clusters: [{ zone: "Dun Morogh", complete: [], ready: [],
                     tooHigh: [{ questId: 309, title: "The Hidden Chamber", level: 22, zone: "Dun Morogh",
                                 standing: "tooHigh", by: 14 }],
                     trivial: [], worthATrip: 0 }],
        bestZone: null,
        log: { used: 3, capacity: 20, free: 17, isTight: false },
        dropCandidates: [], caveats: []
      });
    }
    return JSON.stringify({
      character: "Gravehexx", level: 14,
      turnIns: [{ questId: 788, title: "Cutting Teeth", level: 3, zone: "Elwynn Forest", standing: "trivial", by: 11 }],
      clusters: [
        { zone: "Westfall",
          complete: [],
          ready: [{ questId: 35, title: "Goretusk Liver Pie", level: 14, zone: "Westfall", standing: "ready", by: null },
                  { questId: 22, title: "The Killing Fields", level: 15, zone: "Westfall", standing: "ready", by: null }],
          tooHigh: [], trivial: [], worthATrip: 2 },
        { zone: "Elwynn Forest",
          complete: [{ questId: 788, title: "Cutting Teeth", level: 3, zone: "Elwynn Forest", standing: "trivial", by: 11 }],
          ready: [], tooHigh: [],
          trivial: [{ questId: null, title: "Kobold Candles", level: 4, zone: "Elwynn Forest", standing: "trivial", by: 10 }],
          worthATrip: 1 }
      ],
      bestZone: "Westfall",
      log: { used: 19, capacity: 20, free: 1, isTight: true },
      dropCandidates: [{ questId: null, title: "Kobold Candles", level: 4, zone: "Elwynn Forest", standing: "trivial", by: 10 }],
      caveats: ["Quest levels were not captured for two entries."]
    });
  }
}
