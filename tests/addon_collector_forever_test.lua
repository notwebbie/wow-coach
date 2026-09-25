-- Forever-client test harness for WoWCoachCollector.
--
-- Stubs the Forever API surface using the shapes the live beta actually
-- returned (build 1.60.1.69913). Where a wrong call would still produce a
-- plausible-looking answer, the stub is rigged so it produces a wrong one
-- loudly instead — completion is keyed by questID, so passing a log index
-- fails rather than quietly reading the wrong quest.

local addon_path = assert(arg[1], "usage: lua addon_collector_forever_test.lua <addon.lua>")

local failures = 0
local function check(condition, message)
    if not condition then
        failures = failures + 1
        io.stderr:write("FAIL: " .. message .. "\n")
    end
end

local world = {
    name = "Trollmage", realm = "Classic Beta PvE", level = 24,
    xp = 5600, max_xp = 16400, rested = 24600, money = 104951,
    resting = true, interface = 16001, now = 1700000000,
    quest_header_collapsed = false, skill_header_collapsed = false,
    petName = nil, hardcore = false, pvp = false, rp = false,
}

-- Forever reports as retail. Only the C_SkillInfo namespace distinguishes it.
WOW_PROJECT_ID = 1
WOW_PROJECT_MAINLINE = 1
WOW_PROJECT_CLASSIC = 2

function UnitName(unit)
    if unit == "pet" then return world.petName end
    assert(unit == "player")
    return world.name
end
function UnitExists(unit) return unit == "pet" and world.petName ~= nil or false end
function GetRealmName() return world.realm end
function UnitClass(unit) assert(unit == "player") return "Mage", "MAGE", 8 end
function UnitRace(unit) assert(unit == "player") return "Troll", "Troll", 8 end
function UnitFactionGroup(unit) assert(unit == "player") return "Horde" end
function GetRealZoneText() return "Durotar" end
function GetSubZoneText() return "Valley of Trials" end
function GetBindLocation() return "Sen'jin Village" end
function UnitLevel(unit) assert(unit == "player") return world.level end
function UnitXP(unit) return world.xp end
function UnitXPMax(unit) return world.max_xp end
function GetXPExhaustion() return world.rested end
function IsResting() return world.resting end
function GetMoney() return world.money end
function time() return world.now end
function GetBuildInfo() return "1.60.1", "69913", "Sep 17 2026", world.interface end

-- C_SkillInfo: Forever-exclusive, returns a STRUCTURE, and gives isCollapsed
-- where the Classic client gives isExpanded. tempPoints is non-zero so a
-- collector that adds it (as the default UI does for display) is caught.
C_SkillInfo = {}
function C_SkillInfo.GetNumSkillLines() return 3 end
function C_SkillInfo.GetSkillLineInfo(index)
    if index == 1 then
        return { skillID = 0, name = "Professions", isHeader = true,
                 isCollapsed = world.skill_header_collapsed, rank = 0, maxRank = 0 }
    elseif index == 2 then
        return { skillID = 171, name = "Alchemy", isHeader = false, isCollapsed = false,
                 rank = 225, tempPoints = 10, modifier = 0, maxRank = 300, parentSkillLineID = 0 }
    end
    return { skillID = 182, name = "Herbalism", isHeader = false, isCollapsed = false,
             rank = 150, tempPoints = 0, modifier = 0, maxRank = 300, parentSkillLineID = 0 }
end

-- C_QuestLog: completion is keyed by questID, NOT by log index. IsComplete
-- returns true only for the real questID, so passing the index fails loudly.
C_QuestLog = {}
function C_QuestLog.GetNumQuestLogEntries() return 3, 2 end
function C_QuestLog.GetInfo(index)
    if index == 1 then
        return { title = "Durotar", isHeader = true, isCollapsed = world.quest_header_collapsed }
    elseif index == 2 then
        return { title = "Cutting Teeth", isHeader = false, questID = 788, level = 3 }
    end
    return { title = "Sarkoth", isHeader = false, questID = 795, level = 4 }
end
function C_QuestLog.IsComplete(questID) return questID == 788 end
function C_QuestLog.IsFailed(questID) return questID == 795 end

-- Talents: one trait tree with three groups standing in for the vanilla tabs.
-- The tree resolves from the config's own treeIDs; the spec-index route the
-- beta disproved is deliberately absent here.
C_SpecializationInfo = {}
function C_SpecializationInfo.GetCombatConfigIDForSpecGroup(group) return 6788875 end
C_Traits = {}
function C_Traits.GetConfigInfo(configID)
    if configID ~= 6788875 then return nil end
    return { type = 1, name = "Talents", treeIDs = { 900 } }
end
function C_Traits.GetGroupDisplayInfoByTreeID(treeID)
    if treeID ~= 900 then return nil end
    return { { groupID = 11, displayName = "Arcane" }, { groupID = 12, displayName = "Fire" },
             { groupID = 13, displayName = "Frost" } }
end
function C_Traits.GetGroupCurrencyInfo(configID, groupIDs)
    return { { currencyInfos = { { spent = 0 } } }, { currencyInfos = { { spent = 21 } } },
             { currencyInfos = { { spent = 5 } } } }
end
function C_Traits.GetTreeCurrencyInfo(configID, treeID, exclude)
    return { { quantity = 3, maxQuantity = 51, spent = 26, spentInTree = 26 } }
end

-- Rulesets. Normal is the absence of all three, not a value of its own.
Enum = { GameRule = { HardcoreRuleset = 10, PvPRuleset = 213, RPRuleset = 214,
                      SelfFoundAllowed = 22 } }
C_GameRules = {}
function C_GameRules.IsGameRuleActive(rule)
    if rule == 10 then return world.hardcore end
    if rule == 213 then return world.pvp end
    if rule == 214 then return world.rp end
    return false
end

-- Containers behave as on every current client.
local bag_slots = { [0] = 16, [1] = 6 }
local bag_items = { [0] = { [1] = { 2589, 20 }, [2] = { 2592, 5 } }, [1] = { [1] = { 2589, 12 } } }
C_Container = {}
function C_Container.GetContainerNumSlots(bag) return bag_slots[bag] or 0 end
function C_Container.GetContainerNumFreeSlots(bag)
    local used = 0
    for _ in pairs(bag_items[bag] or {}) do used = used + 1 end
    return (bag_slots[bag] or 0) - used, 0
end
function C_Container.GetContainerItemInfo(bag, slot)
    local entry = (bag_items[bag] or {})[slot]
    if not entry then return nil end
    return { itemID = entry[1], stackCount = entry[2] }
end
function C_Container.ContainerIDToInventoryID(bag) return 19 + bag end
function GetInventoryItemID(unit, slot) return 4000 + slot end

-- Trade skills: retail-shaped, gated on IsTradeSkillReady.
local trade_ready = true
C_TradeSkillUI = {}
function C_TradeSkillUI.IsTradeSkillReady() return trade_ready end
function C_TradeSkillUI.GetBaseProfessionInfo()
    if not trade_ready then return nil end
    return { professionName = "Alchemy", professionID = 171, skillLevel = 225, maxSkillLevel = 300 }
end
function C_TradeSkillUI.GetAllRecipeIDs()
    if not trade_ready then return {} end
    return { 2330, 2337 }
end
function C_TradeSkillUI.GetRecipeInfo(recipeID)
    local names = { [2330] = "Minor Healing Potion", [2337] = "Lesser Healing Potion" }
    return { name = names[recipeID], relativeDifficulty = "optimal", categoryID = 1 }
end

local frame = { events = {}, scripts = {} }
function frame:RegisterEvent(event) self.events[event] = true end
function frame:SetScript(script, handler) self.scripts[script] = handler end
function frame:Fire(event, ...)
    check(self.events[event], "event not registered: " .. event)
    if self.events[event] and self.scripts.OnEvent then self.scripts.OnEvent(self, event, ...) end
end
function frame:Tick(elapsed) if self.scripts.OnUpdate then self.scripts.OnUpdate(self, elapsed) end end
function CreateFrame(kind) assert(kind == "Frame") return frame end

local addon_chunk = assert(loadfile(addon_path))
addon_chunk("WoWCoachCollector")
local collector = assert(_G.WoWCoachCollector, "addon did not expose its namespace")

local key = collector.characterKey("forever", world.realm, world.name)
local function record() return assert(WoWCoachCollectorDB.characters[key], "no snapshot at " .. key) end

--------------------------------------------------------------------------------

frame:Fire("PLAYER_LOGIN")
local snap = record()

check(snap.gameFlavor == "forever", "the C_SkillInfo sentinel should identify Forever")
check(snap.interfaceVersion == 16001, "raw interface number should still be stored")
check(snap.schemaVersion == 2, "Forever support is additive; schema stays at 2")
check(snap.name == "Trollmage" and snap.realm == "Classic Beta PvE", "identity captured")
check(snap.level == 24 and snap.class == "MAGE", "level and class captured")
check(snap.restedXP == 24600 and snap.isResting == true, "rested state captured")

-- Skills: same shape as the TBC path, from a different API.
check(snap.skills and #snap.skills == 2, "headers excluded from skills")
local alchemy
for _, s in ipairs(snap.skills or {}) do if s.name == "Alchemy" then alchemy = s end end
check(alchemy and alchemy.rank == 225, "rank must be raw, without tempPoints")
check(alchemy and alchemy.maxRank == 300, "maxRank captured")
check(alchemy and alchemy.skillID == 171, "Forever also gives a stable skillID")
check(snap.skillsComplete == true, "expanded headers mean a complete capture")

-- Quests: completion keyed by questID, and headers tracked.
check(snap.quests and #snap.quests == 2, "headers excluded from quests")
check(snap.quests and snap.quests[1].questID == 788, "questID captured")
check(snap.quests and snap.quests[1].header == "Durotar", "header tracked")
check(snap.quests and snap.quests[1].state == "complete", "IsComplete must be called with questID")
check(snap.quests and snap.quests[2].state == "failed", "IsFailed must be called with questID")
check(snap.quests and snap.quests[1].difficulty == nil, "collector must not derive difficulty")

-- Talents via traits.
check(snap.talents and #snap.talents.trees == 3, "three trait groups captured")
local fire
for _, tree in ipairs(snap.talents and snap.talents.trees or {}) do
    if tree.name == "Fire" then fire = tree end
end
check(fire and fire.pointsSpent == 21, "pointsSpent from group currency info")
check(snap.talents and snap.talents.unspentPoints == 3, "unspent points from tree currency")

-- Ruleset: Normal is all three false, and false is recorded, not omitted.
check(snap.ruleset ~= nil, "ruleset captured on Forever")
check(snap.ruleset and snap.ruleset.hardcore == false, "hardcore recorded as false, not dropped")
check(snap.ruleset and snap.ruleset.pvp == false, "pvp recorded as false")
check(snap.ruleset and snap.ruleset.rp == false, "rp recorded as false")

-- Inventory, shared path.
check(snap.inventory and snap.inventory.totalSlots == 22, "bag slots summed")
local linen
for _, item in ipairs(snap.inventory and snap.inventory.contents or {}) do
    if item.itemID == 2589 then linen = item end
end
check(linen and linen.count == 32, "stacks summed across bags")

-- No pet on a mage.
check(snap.pet == nil, "no pet should mean no pet record")

--------------------------------------------------------------------------------
-- A future patch moves the interface number
--
-- The whole point of the C_SkillInfo sentinel is that interface numbers change.
-- Pin that: with a number outside every known range, the flavor must still come
-- out as forever, and the snapshot must land under the forever key.
--------------------------------------------------------------------------------

world.interface = 30500
frame:Fire("PLAYER_ENTERING_WORLD")
check(record().gameFlavor == "forever",
    "an interface number outside all known ranges must still resolve via the sentinel")
check(record().interfaceVersion == 30500, "the new raw interface number is stored")
-- Without the sentinel the same client reads as retail, because that is what
-- it reports. That is the whole failure the sentinel exists to prevent.
check(collector.gameFlavor(30500, false) == "retail",
    "without the sentinel a Forever client is indistinguishable from retail")
world.interface = 16001
frame:Fire("PLAYER_ENTERING_WORLD")

--------------------------------------------------------------------------------
-- Partial captures and ruleset changes
--------------------------------------------------------------------------------

world.quest_header_collapsed = true
world.skill_header_collapsed = true
world.hardcore = true
frame:Fire("QUEST_LOG_UPDATE")
frame:Tick(10)
check(record().questsComplete == false, "collapsed quest header means partial")
check(record().skillsComplete == false, "collapsed skill header means partial (isCollapsed, not isExpanded)")
check(record().ruleset.hardcore == true, "ruleset change picked up")
world.quest_header_collapsed = false
world.skill_header_collapsed = false
world.hardcore = false

--------------------------------------------------------------------------------
-- Recipes
--------------------------------------------------------------------------------

frame:Fire("TRADE_SKILL_LIST_UPDATE")
local recipes = record().recipes
check(recipes and recipes.Alchemy, "trade skill stored under the profession")
check(recipes and recipes.Alchemy and recipes.Alchemy.rank == 225, "profession rank stored")
check(recipes and recipes.Alchemy and #recipes.Alchemy.recipes == 2, "recipes enumerated")
check(recipes and recipes.Alchemy and recipes.Alchemy.recipes[1].spellID == 2330,
    "recipe id retained")
check(recipes and recipes.Alchemy and recipes.Alchemy.capturedAt == world.now,
    "recipes are a cache and must be timestamped")

-- With the window closed the API is not ready; the cached data must survive.
trade_ready = false
frame:Fire("TRADE_SKILL_LIST_UPDATE")
check(record().recipes.Alchemy ~= nil, "an unready trade skill must not wipe the cache")
trade_ready = true

--------------------------------------------------------------------------------
-- Pet
--------------------------------------------------------------------------------

world.petName = "Snarlfang"
C_PetInfo = {
    GetPetHappiness = function() return 3, 1.25, 6 end,
    GetPetLoyalty = function() return 6 end,
    GetPetTrainingPoints = function() return 40, 100 end,
}
frame:Fire("UNIT_PET")
frame:Tick(10)
local pet = record().pet
check(pet ~= nil, "pet captured when one is out")
check(pet and pet.name == "Snarlfang", "pet name captured")
check(pet and pet.happiness == 3, "pet happiness captured")
check(pet and pet.trainingPointsSpent == 40, "training points captured")

-- The live beta showed these return NO values with no pet out, not nil.
world.petName = nil
C_PetInfo.GetPetHappiness = function() return end
C_PetInfo.GetPetLoyalty = function() return end
C_PetInfo.GetPetTrainingPoints = function() return end
local ok, err = pcall(collector.capture)
check(ok, "capture must survive pet APIs returning no values: " .. tostring(err))

--------------------------------------------------------------------------------

if failures > 0 then
    io.stderr:write(string.format("%d assertion(s) failed\n", failures))
    os.exit(1)
end
print("forever collector tests passed")
