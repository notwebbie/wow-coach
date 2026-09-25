-- Test harness for WoWCoachCollector.
--
-- Stubs the subset of the WoW API the collector calls, using the signatures
-- and return orders of the Classic 2.5.x-lineage client. Where a return order
-- is load-bearing (GetSkillLineInfo rank/maxRank, GetQuestLogTitle questID)
-- the stub returns deliberately distinct values so a mis-indexed read fails
-- loudly instead of returning a plausible number.

local addon_path = assert(arg[1], "usage: lua addon_collector_test.lua <addon.lua>")

local failures = 0
local function check(condition, message)
    if not condition then
        failures = failures + 1
        io.stderr:write("FAIL: " .. message .. "\n")
    end
end

--------------------------------------------------------------------------------
-- Mutable world state
--------------------------------------------------------------------------------

local world = {
    name = "Testhero",
    realm = "Test Realm",
    level = 34,
    xp = 3210,
    max_xp = 7800,
    rested = 900,
    money = 456789,
    resting = true,
    interface = 20506,
    now = 1700000000,
    quest_header_collapsed = false,
    skill_header_expanded = true,
    petName = nil,
}

WOW_PROJECT_ID = 5
WOW_PROJECT_CLASSIC = 2
WOW_PROJECT_BURNING_CRUSADE_CLASSIC = 5
WOW_PROJECT_MAINLINE = 1

function UnitName(unit)
    if unit == "pet" then return world.petName end
    assert(unit == "player")
    return world.name
end
function UnitExists(unit) return unit == "pet" and world.petName ~= nil or false end
function GetRealmName() return world.realm end
function UnitClass(unit) assert(unit == "player") return "Mage", "MAGE", 8 end
function UnitRace(unit) assert(unit == "player") return "Human", "Human", 1 end
function UnitFactionGroup(unit) assert(unit == "player") return "Alliance" end
function GetRealZoneText() return "Test Zone" end
function GetSubZoneText() return "Test Subzone" end
function GetBindLocation() return "Test Inn" end
function UnitLevel(unit) assert(unit == "player") return world.level end
function UnitXP(unit) assert(unit == "player") return world.xp end
function UnitXPMax(unit) assert(unit == "player") return world.max_xp end
function GetXPExhaustion() return world.rested end
function IsResting() return world.resting end
function GetMoney() return world.money end
function time() return world.now end
function GetBuildInfo() return "2.5.6", "69795", "Sep 12 2026", world.interface end

-- Skills: header, then two professions. Rank is return 4, maxRank return 7.
-- numTempPoints (return 5) is non-zero so that a collector which wrongly adds
-- it, as Blizzard's display code does, produces a detectably wrong rank.
function GetNumSkillLines() return 3 end
function GetSkillLineInfo(index)
    if index == 1 then
        return "Professions", true, world.skill_header_expanded, 0, 0, 0, 0
    elseif index == 2 then
        return "Alchemy", false, true, 225, 10, 0, 300
    end
    return "Herbalism", false, true, 150, 0, 0, 300
end

-- Talents: pointsSpent is return 7 of C_SpecializationInfo.GetSpecializationInfo.
C_SpecializationInfo = {}
function C_SpecializationInfo.GetSpecializationInfo(index)
    local names = { "Arcane", "Fire", "Frost" }
    local spent = { 0, 31, 5 }
    return 100 + index, names[index], "desc", "icon", "role", "stat", spent[index], "bg", 0, true
end
function GetNumTalentTabs() return 3 end
function GetUnspentTalentPoints() return 2 end

-- Quest log: header, two quests. questID is return 8, isComplete is a NUMBER.
function GetNumQuestLogEntries() return 3, 2 end
function GetQuestLogTitle(index)
    if index == 1 then
        return "Hellfire Peninsula", 0, nil, true, world.quest_header_collapsed, nil, 0, 0
    elseif index == 2 then
        return "Cleansing the Waters", 62, nil, false, false, 1, 0, 9440
    end
    return "Failed Errand", 60, nil, false, false, -1, 0, 9441
end

-- Containers: backpack plus one bag. Modern C_Container only; the globals are
-- absent, matching the 2.5.x client.
local bag_slots = { [0] = 16, [1] = 6 }
local bag_items = {
    [0] = { [1] = { 2589, 20 }, [2] = { 2592, 5 } },
    [1] = { [1] = { 2589, 12 } },
}
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
function GetInventoryItemID(unit, slot) assert(unit == "player") return 4000 + slot end

-- Trade skill and craft.
local trade_open = true
function GetTradeSkillLine()
    if not trade_open then return "UNKNOWN", 0, 0 end
    return "Alchemy", 225, 300
end
function GetNumTradeSkills() return trade_open and 3 or 0 end
function GetTradeSkillInfo(index)
    if not trade_open then return nil end
    if index == 1 then return "Potions", "header" end
    if index == 2 then return "Elixir of Fortitude", "optimal" end
    return "Healing Potion", "trivial"
end
function GetTradeSkillRecipeLink(index)
    return "|cffffd000|Hspell:" .. (11450 + index) .. "|h[Recipe]|h|r"
end

local craft_open = false
function GetCraftDisplaySkillLine()
    if not craft_open then return "" end
    return "Enchanting", 180, 300
end
function GetNumCrafts() return craft_open and 2 or 0 end
function GetCraftInfo(index)
    if not craft_open then return nil end
    if index == 1 then return "Enchantments", nil, "header" end
    return "Enchant Bracer - Minor Health", nil, "easy", 1, false, 0, 1
end
function GetCraftRecipeLink(index)
    return "|cffffd000|Henchant:" .. (7418 + index) .. "|h[Enchant]|h|r"
end

--------------------------------------------------------------------------------
-- Frame stub
--------------------------------------------------------------------------------

local frame = { events = {}, scripts = {} }
function frame:RegisterEvent(event) self.events[event] = true end
function frame:SetScript(script, handler) self.scripts[script] = handler end
function frame:Fire(event, ...)
    check(self.events[event], "event not registered: " .. event)
    if self.events[event] and self.scripts.OnEvent then
        self.scripts.OnEvent(self, event, ...)
    end
end
function frame:Tick(elapsed)
    if self.scripts.OnUpdate then self.scripts.OnUpdate(self, elapsed) end
end
function CreateFrame(kind) assert(kind == "Frame") return frame end

--------------------------------------------------------------------------------
-- Load
--------------------------------------------------------------------------------

local addon_chunk = assert(loadfile(addon_path))
addon_chunk("WoWCoachCollector")
local collector = assert(_G.WoWCoachCollector, "addon did not expose its namespace")

local key = collector.characterKey("tbc_classic", world.realm, world.name)

local function record()
    return assert(WoWCoachCollectorDB.characters[key], "no snapshot at key " .. key)
end

--------------------------------------------------------------------------------
-- Flavor detection
--------------------------------------------------------------------------------

check(collector.gameFlavor(20506) == "tbc_classic", "20506 should be tbc_classic")
check(collector.gameFlavor(11404) == "classic_era", "11404 should be classic_era")
check(collector.gameFlavor(120100) == "retail", "120100 should be retail")
-- WoW Forever reports WOW_PROJECT_MAINLINE, so only the interface number can
-- tell it apart from retail. 16001 must not fall through to classic_era.
check(collector.gameFlavor(16001) == "forever", "16001 should be forever, not classic_era")
-- The sentinel outranks the interface number: Forever reports MAINLINE, and its
-- interface number changes with every patch, so an unseen number must still
-- resolve correctly when the Forever-only namespace is present.
WOW_PROJECT_ID = 1
check(collector.gameFlavor(16099, true) == "forever",
    "an unseen interface number with C_SkillInfo present should be forever")
check(collector.gameFlavor(120100, false) == "retail",
    "MAINLINE without C_SkillInfo is retail")
WOW_PROJECT_ID = 5

--------------------------------------------------------------------------------
-- Login snapshot
--------------------------------------------------------------------------------

frame:Fire("PLAYER_LOGIN")
local snap = record()

check(snap.schemaVersion == 2, "schema version should be 2")
check(snap.capturedAt == world.now, "capturedAt should be the epoch time")
check(snap.gameFlavor == "tbc_classic", "flavor should be tbc_classic")
check(snap.interfaceVersion == 20506, "raw interface number should be stored")
check(snap.name == "Testhero" and snap.realm == "Test Realm", "identity should be captured")
check(snap.level == 34, "level should be captured")
check(snap.class == "MAGE" and snap.classID == 8, "class file and id should be captured")
check(snap.race == "Human" and snap.raceID == 1, "race file and id should be captured")
check(snap.faction == "Alliance", "faction should be captured")
check(snap.zone == "Test Zone" and snap.subZone == "Test Subzone", "zone should be captured")
check(snap.bindLocation == "Test Inn", "bind location should be captured")
check(snap.isResting == true, "resting state should be captured")
check(snap.xp == 3210 and snap.maxXP == 7800, "xp should be captured")
check(snap.restedXP == 900, "rested xp should be captured")
check(snap.moneyCopper == 456789, "money should be captured")

-- Skills
check(snap.skills and #snap.skills == 2, "headers should be excluded from skills")
local alchemy
for _, skill in ipairs(snap.skills or {}) do
    if skill.name == "Alchemy" then alchemy = skill end
end
check(alchemy ~= nil, "Alchemy should be captured")
check(alchemy and alchemy.rank == 225, "rank must come from return 4 without numTempPoints")
check(alchemy and alchemy.maxRank == 300, "maxRank must come from return 7")
check(snap.skillsComplete == true, "expanded headers mean a complete skill capture")

-- Talents
check(snap.talents and #snap.talents.trees == 3, "three talent trees should be captured")
local fire
for _, tree in ipairs(snap.talents and snap.talents.trees or {}) do
    if tree.name == "Fire" then fire = tree end
end
check(fire and fire.pointsSpent == 31, "pointsSpent must come from return 7")
check(snap.talents and snap.talents.unspentPoints == 2, "unspent points should be captured")

-- Quests
check(snap.quests and #snap.quests == 2, "headers should be excluded from quests")
check(snap.quests and snap.quests[1].questID == 9440, "questID must come from return 8")
check(snap.quests and snap.quests[1].title == "Cleansing the Waters", "quest title should be captured")
check(snap.quests and snap.quests[1].level == 62, "quest level should be captured")
check(snap.quests and snap.quests[1].header == "Hellfire Peninsula", "quest header should be tracked")
check(snap.quests and snap.quests[1].state == "complete", "isComplete > 0 means complete")
check(snap.quests and snap.quests[2].state == "failed", "isComplete < 0 means failed")
check(snap.questsComplete == true, "expanded headers mean a complete quest capture")
-- Difficulty is a judgement, not a fact, and belongs in the core.
check(snap.quests and snap.quests[1].difficulty == nil, "collector must not derive quest difficulty")

-- Bags
check(snap.inventory ~= nil, "inventory should be captured")
check(snap.inventory and snap.inventory.totalSlots == 22, "total slots should sum every bag")
check(snap.inventory and snap.inventory.freeSlots == 19, "free slots should sum every bag")
check(snap.inventory and #snap.inventory.bags == 2, "both bags should be listed")
local linen
for _, item in ipairs(snap.inventory and snap.inventory.contents or {}) do
    if item.itemID == 2589 then linen = item end
end
check(linen and linen.count == 32, "stacks of one item must be summed across bags")

--------------------------------------------------------------------------------
-- Throttling
--------------------------------------------------------------------------------

world.money = 999999
frame:Fire("BAG_UPDATE_DELAYED")
check(record().moneyCopper == 456789, "burst events must not capture immediately")
frame:Tick(1)
check(record().moneyCopper == 456789, "capture must not flush before the throttle elapses")
frame:Tick(5)
check(record().moneyCopper == 999999, "capture must flush once the throttle elapses")

-- A second tick with nothing pending must not re-capture.
world.money = 111111
frame:Tick(10)
check(record().moneyCopper == 999999, "idle ticks must not capture")

-- XP updates for other units are not ours.
world.money = 222222
frame:Fire("PLAYER_XP_UPDATE", "target")
frame:Tick(10)
check(record().moneyCopper == 999999, "non-player XP updates must not mark state dirty")

frame:Fire("PLAYER_XP_UPDATE", "player")
frame:Tick(10)
check(record().moneyCopper == 222222, "player XP updates must mark state dirty")

--------------------------------------------------------------------------------
-- Partial captures
--------------------------------------------------------------------------------

world.quest_header_collapsed = true
world.skill_header_expanded = false
frame:Fire("QUEST_LOG_UPDATE")
frame:Tick(10)
check(record().questsComplete == false, "a collapsed quest header means a partial capture")
check(record().skillsComplete == false, "a collapsed skill header means a partial capture")
world.quest_header_collapsed = false
world.skill_header_expanded = true

--------------------------------------------------------------------------------
-- Recipes
--------------------------------------------------------------------------------

frame:Fire("TRADE_SKILL_SHOW")
local recipes = record().recipes
check(recipes and recipes.Alchemy, "trade skill capture should store under the profession")
check(recipes and recipes.Alchemy and recipes.Alchemy.rank == 225, "profession rank should be stored")
check(recipes and recipes.Alchemy and #recipes.Alchemy.recipes == 2, "recipe headers should be excluded")
check(recipes and recipes.Alchemy and recipes.Alchemy.recipes[1].difficulty == "optimal",
    "recipe difficulty should be stored")
check(recipes and recipes.Alchemy and recipes.Alchemy.recipes[1].spellID == 11452,
    "spell id should be parsed out of the recipe link")
check(recipes and recipes.Alchemy and recipes.Alchemy.capturedAt == world.now,
    "recipes are a cache and must be timestamped")

-- TBC keeps Enchanting on the separate Craft API.
trade_open = false
craft_open = true
frame:Fire("CRAFT_SHOW")
recipes = record().recipes
check(recipes and recipes.Enchanting, "craft capture should store Enchanting")
check(recipes and recipes.Enchanting and #recipes.Enchanting.recipes == 1, "craft headers should be excluded")
check(recipes and recipes.Enchanting and recipes.Enchanting.recipes[1].spellID == 7420,
    "enchant link id should be parsed")
check(recipes and recipes.Alchemy, "a craft capture must not discard trade skill data")
trade_open = true
craft_open = false

--------------------------------------------------------------------------------
-- Degradation when APIs are absent
--------------------------------------------------------------------------------

local saved = {
    C_Container = C_Container,
    GetNumSkillLines = GetNumSkillLines,
    GetNumTalentTabs = GetNumTalentTabs,
    GetXPExhaustion = GetXPExhaustion,
}
C_Container = nil
GetNumSkillLines = nil
GetNumTalentTabs = nil
GetXPExhaustion = nil

local before = record().skills
local ok, err = pcall(collector.capture)
check(ok, "capture must not error when APIs are missing: " .. tostring(err))
check(record().level == 34, "capture should still record what it can")
check(record().skills == before, "a missing API must not clobber previously captured data")

C_Container = saved.C_Container
GetNumSkillLines = saved.GetNumSkillLines
GetNumTalentTabs = saved.GetNumTalentTabs
GetXPExhaustion = saved.GetXPExhaustion

--------------------------------------------------------------------------------

if failures > 0 then
    io.stderr:write(string.format("%d assertion(s) failed\n", failures))
    os.exit(1)
end
print("addon collector tests passed")

--------------------------------------------------------------------------------
-- Fixture generation
--
-- With a second argument, dump the resulting database in the format WoW's own
-- SavedVariables serializer uses, so the Rust parser in wow-coach-core can be
-- tested against a realistic file without anyone having to play the game.
-- Keys are sorted, which the real writer does not do, so the fixture is
-- reproducible; readers must not depend on key order.
--------------------------------------------------------------------------------

local function serialize(value, indent)
    local t = type(value)
    if t == "number" or t == "boolean" then return tostring(value) end
    if t == "string" then return string.format("%q", value) end
    if t ~= "table" then error("value outside the restricted subset: " .. t) end

    local numeric, strings = {}, {}
    for key in pairs(value) do
        if type(key) == "number" then numeric[#numeric + 1] = key
        elseif type(key) == "string" then strings[#strings + 1] = key
        else error("table key outside the restricted subset: " .. type(key)) end
    end
    table.sort(numeric)
    table.sort(strings)

    local pad, inner = string.rep("\t", indent), string.rep("\t", indent + 1)
    local parts = {}
    for _, key in ipairs(numeric) do
        parts[#parts + 1] = inner .. "[" .. key .. "] = " .. serialize(value[key], indent + 1) .. ","
    end
    for _, key in ipairs(strings) do
        parts[#parts + 1] = inner .. "[" .. string.format("%q", key) .. "] = " ..
            serialize(value[key], indent + 1) .. ","
    end
    if #parts == 0 then return "{\n" .. pad .. "}" end
    return "{\n" .. table.concat(parts, "\n") .. "\n" .. pad .. "}"
end

if arg[2] then
    local handle = assert(io.open(arg[2], "w"))
    handle:write("WoWCoachCollectorDB = " .. serialize(WoWCoachCollectorDB, 0) .. "\n")
    handle:close()
    print("wrote fixture to " .. arg[2])
end
