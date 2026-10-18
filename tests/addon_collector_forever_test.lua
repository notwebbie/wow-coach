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

-- GetProfessions: sparse on purpose, in the shape the live beta returned
-- (6,7,5,nil,8,nil on build 70235). The hole at slot 4 is the trap — a reader
-- that walks this with ipairs stops there and silently loses Cooking, which is
-- exactly how the probe that found this API under-reported. Index 5 resolves
-- to First Aid in the slot the documented signature calls archaeology, so a
-- reader that trusts position over GetProfessionInfo gets a wrong name rather
-- than a missing one.
function GetProfessions() return 6, 7, 5, nil, 8, nil end
function GetProfessionInfo(index)
    if index == 5 then return "First Aid", nil, 72, 150, 0, 0, 129 end
    if index == 6 then return "Alchemy", nil, 225, 300, 0, 0, 171 end
    if index == 7 then return "Herbalism", nil, 150, 300, 0, 0, 182 end
    if index == 8 then return "Cooking", nil, 40, 75, 0, 0, 185 end
    return nil
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

-- Per-node detail. Coordinates are pixels, deliberately unevenly spaced and
-- listed out of order, so the collector has to rank them rather than divide.
-- Node 505 has a point placed but not applied (currentRank 1, purchased 0):
-- only committed ranks may be recorded. Node 509 belongs to a group that is
-- not one of the three tabs and must be ignored. Node 0 is the API's empty
-- slot marker.
local trait_nodes = {
    [501] = { ID = 501, posX = 1800, posY = 1170, groupIDs = { 13 }, ranksPurchased = 0,
              currentRank = 0, maxRanks = 2, entryIDs = { 601 } },
    [502] = { ID = 502, posX = 2400, posY = 1200, groupIDs = { 13 }, ranksPurchased = 5,
              currentRank = 5, maxRanks = 5, entryIDs = { 602 }, activeEntry = { entryID = 602, rank = 5 },
              visibleEdges = { { targetNode = 504, type = 1, isActive = true } } },
    [503] = { ID = 503, posX = 1815, posY = 1985, groupIDs = { 13, 77 }, ranksPurchased = 0,
              currentRank = 0, maxRanks = 5, entryIDs = { 603 } },
    [504] = { ID = 504, posX = 3900, posY = 1950, groupIDs = { 13 }, ranksPurchased = 0,
              currentRank = 0, maxRanks = 3, entryIDs = { 604 } },
    [505] = { ID = 505, posX = 1800, posY = 1200, groupIDs = { 12 }, ranksPurchased = 0,
              currentRank = 1, maxRanks = 5, entryIDs = { 605 } },
    [509] = { ID = 509, posX = 100, posY = 100, groupIDs = { 99 }, ranksPurchased = 1,
              currentRank = 1, maxRanks = 1, entryIDs = { 609 } },
}
function C_Traits.GetTreeNodes(treeID)
    if treeID ~= 900 then return {} end
    return { 504, 509, 503, 0, 502, 505, 501 }
end
function C_Traits.GetNodeInfo(configID, nodeID)
    return trait_nodes[nodeID] or { ID = 0 }
end
function C_Traits.GetEntryInfo(configID, entryID)
    return { definitionID = entryID + 100, maxRanks = 1 }
end
local trait_definitions = {
    [701] = { spellID = 11189 }, [702] = { spellID = 16766 }, [703] = { spellID = 11207 },
    [704] = { spellID = 11175, overrideName = "Permafrost" },
    [705] = { spellID = 11069 }, [709] = { spellID = 1 },
}
function C_Traits.GetDefinitionInfo(definitionID) return trait_definitions[definitionID] end
local spell_names = { [11189] = "Frost Warding", [16766] = "Improved Frostbolt",
                      [11207] = "Ice Shards", [11069] = "Improved Fireball", [1] = "Hidden" }
C_Spell = C_Spell or {}
function C_Spell.GetSpellName(spellID) return spell_names[spellID] end

-- Legacy trees. Three states, because all three happen:
--   1187 professions      -- no configID: the client will not hand one over.
--                            Absent, which is NOT the same as nothing bought.
--   1188 adventure        -- readable, one node bought, one node whose name
--                            exists only as a spell name. The probe read
--                            `overrideName` alone and got nil for every node,
--                            so a reader that regresses to that fails here.
--   1189 resourcefulness  -- readable, nothing bought.
local legacy_nodes = {
    [110296] = { ID = 110296, ranksPurchased = 0, maxRanks = 5, entryIDs = { 801 } },
    [110301] = { ID = 110301, ranksPurchased = 2, maxRanks = 5, entryIDs = { 802 },
                 activeEntry = { entryID = 802, rank = 2 } },
    [110300] = { ID = 110300, ranksPurchased = 0, maxRanks = 1, entryIDs = { 803 } },
    [119001] = { ID = 119001, ranksPurchased = 0, maxRanks = 3, entryIDs = { 804 } },
}
local legacy_tree_nodes = {
    [1188] = { 110296, 110301, 110300 },
    [1189] = { 119001 },
}
function C_Traits.GetConfigIDByTreeID(treeID)
    if treeID == 1188 then return 7001 end
    if treeID == 1189 then return 7002 end
    return nil  -- 1187 refuses
end
do
    local baseTreeNodes = C_Traits.GetTreeNodes
    function C_Traits.GetTreeNodes(treeID)
        if legacy_tree_nodes[treeID] then return legacy_tree_nodes[treeID] end
        return baseTreeNodes(treeID)
    end
    local baseNodeInfo = C_Traits.GetNodeInfo
    function C_Traits.GetNodeInfo(configID, nodeID)
        if legacy_nodes[nodeID] then return legacy_nodes[nodeID] end
        return baseNodeInfo(configID, nodeID)
    end
    local baseDefinition = C_Traits.GetDefinitionInfo
    function C_Traits.GetDefinitionInfo(definitionID)
        -- 901 carries an overrideName; the rest resolve only through the spell
        -- name, which is the case the probe got wrong.
        if definitionID == 901 then return { spellID = 450001, overrideName = "Seasoned Traveller" } end
        if definitionID == 902 then return { spellID = 1225478 } end
        if definitionID == 903 then return { spellID = 450003 } end
        if definitionID == 904 then return { spellID = 450004 } end
        return baseDefinition(definitionID)
    end
    local baseCurrency = C_Traits.GetTreeCurrencyInfo
    function C_Traits.GetTreeCurrencyInfo(configID, treeID, exclude)
        if treeID == 1188 then return { { quantity = 1, maxQuantity = 9, spentInTree = 2 } } end
        if treeID == 1189 then return { { quantity = 0, maxQuantity = 4, spentInTree = 0 } } end
        return baseCurrency(configID, treeID, exclude)
    end
end
spell_names[1225478] = "Well Rested"
spell_names[450003] = "Pathfinder"
spell_names[450004] = "Scavenger"

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
    return { 2330, 2337, 9999 }
end
-- relativeDifficulty is Enum.TradeskillRelativeDifficulty: an INTEGER, where
-- the Classic path returns a word. This stub said "optimal" until the live
-- beta proved otherwise, and that lie is why the harness passed while the real
-- client produced a file the reader rejected outright. 0 Optimal, 1 Medium,
-- 2 Easy, 3 Trivial; 99 stands in for a value a later client might add.
local recipe_difficulty = { [2330] = 0, [2337] = 3, [9999] = 99 }
function C_TradeSkillUI.GetRecipeInfo(recipeID)
    local names = { [2330] = "Minor Healing Potion", [2337] = "Lesser Healing Potion",
        [9999] = "Recipe From A Later Client" }
    return {
        name = names[recipeID],
        relativeDifficulty = recipe_difficulty[recipeID],
        categoryID = 1,
    }
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
local frost_trait
for _, tree in ipairs(snap.talents and snap.talents.trees or {}) do
    if tree.name == "Frost" then frost_trait = tree end
    if tree.name == "Arcane" then check(tree.talents == nil, "a group with no nodes stores no list") end
end
local ft = frost_trait and frost_trait.talents or {}
check(#ft == 4, "only the group's own nodes captured; empty slots and other groups ignored")
check(ft[1] and ft[1].name == "Frost Warding" and ft[1].tier == 1 and ft[1].column == 1,
    "pixel positions ranked into tier 1, column 1")
check(ft[2] and ft[2].name == "Improved Frostbolt" and ft[2].column == 2 and ft[2].rank == 5,
    "second column and committed rank")
check(ft[2] and ft[2].spellID == 16766, "spellID kept as a stable identity")
check(ft[3] and ft[3].name == "Ice Shards" and ft[3].tier == 2, "second distinct row is tier 2")
check(ft[1] and ft[1].tier == ft[2].tier, "a node drawn slightly higher is still in its row")
check(ft[3] and ft[3].column == 1, "a node drawn slightly right is still in its column")
check(ft[2] and ft[2].posX == 2400 and ft[2].posY == 1200, "raw coordinates kept")
check(ft[4] and ft[4].name == "Permafrost", "overrideName preferred over the spell name")
check(ft[4] and ft[4].column == 3, "columns ranked over the whole group, not per row")
check(ft[4] and ft[4].requires and ft[4].requires.tier == 1 and ft[4].requires.column == 2,
    "an edge into a node becomes its prerequisite")
local fire_trait
for _, tree in ipairs(snap.talents and snap.talents.trees or {}) do
    if tree.name == "Fire" then fire_trait = tree end
end
local fireball = fire_trait and fire_trait.talents and fire_trait.talents[1]
check(fireball and fireball.rank == 0, "a placed but unapplied point is not a purchased rank")
check(fireball and fireball.maxRank == 5, "maxRank from the node")

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
-- Legacy
--
-- The point of this capture is one question: has this character bought
-- anything that changes the rested model? "Every node reads zero" answers it
-- without knowing which node is which.
--------------------------------------------------------------------------------

local legacy = {}
for _, tree in ipairs(snap.legacy or {}) do legacy[tree.tree] = tree end

check(snap.legacy ~= nil and #snap.legacy == 3, "all three Legacy trees are reported")
check(legacy.professions ~= nil and legacy.professions.configID == nil,
    "a tree the client refuses a configID for is reported as present but unreadable")
check(legacy.professions and legacy.professions.nodes == nil,
    "and carries no node list, because absent is not the same as nothing bought")

check(legacy.adventure and legacy.adventure.configID == 7001, "the readable tree carries its configID")
check(legacy.adventure and legacy.adventure.spent == 2, "and what has been spent in it")
check(legacy.adventure and #legacy.adventure.nodes == 3, "every node in the tree")

local byNode = {}
for _, node in ipairs(legacy.adventure and legacy.adventure.nodes or {}) do byNode[node.nodeID] = node end
check(byNode[110301] and byNode[110301].rank == 2, "ranks purchased is the fact that matters")
check(byNode[110296] and byNode[110296].rank == 0, "and an untouched node reads zero, not nil")

-- The probe read `overrideName` only and got nil for all nine live nodes.
check(byNode[110301] and byNode[110301].name == "Well Rested",
    "a node with no overrideName resolves through its spell name")
check(byNode[110301] and byNode[110301].spellID == 1225478,
    "and carries the spell id, which is what the core matches on")
check(byNode[110296] and byNode[110296].name == "Seasoned Traveller",
    "and a node that does override its spell name keeps the override")

check(legacy.resourcefulness and legacy.resourcefulness.spent == 0,
    "a readable tree with nothing bought reports zero spent, which is an answer")

--------------------------------------------------------------------------------
-- Professions
--
-- The point of this capture is to tell a trade skill from a language without
-- reading a name, so the assertions are about completeness and identity, not
-- about where in the return each one sat.
--------------------------------------------------------------------------------

local slots = snap.professions
check(slots ~= nil, "professions captured where GetProfessions exists")
check(slots and #slots == 4,
    "every non-nil slot is read, including the one past the hole (ipairs would stop at 3)")

local byName = {}
for _, slot in ipairs(slots or {}) do byName[slot.name] = slot end
check(byName.Cooking ~= nil, "Cooking sits after the nil at slot 4 and must survive it")
check(byName["First Aid"] and byName["First Aid"].skillLine == 129,
    "the skill line id is read from GetProfessionInfo, not inferred from the slot")
check(byName["First Aid"] and byName["First Aid"].slot == 3,
    "the slot is recorded as a fact — position 3 holds First Aid on this client")
check(byName.Herbalism and byName.Herbalism.rank == 150 and byName.Herbalism.maxRank == 300,
    "rank comes from GetProfessionInfo, which answers for gathering skills too")
check(byName.Herbalism and byName.Herbalism.index == 7, "the index passed to GetProfessionInfo is kept")
-- Herbalism is the whole reason this exists: it is a profession here and the
-- skill list cannot say so.
check(byName.Alchemy ~= nil and byName.Herbalism ~= nil,
    "both primaries present")
-- The arity is the fact that makes an empty list readable. GetProfessions
-- returned six values here, four of them real.
check(snap.professionsReturned == 6,
    "how many values the client returned is recorded, nils counted")

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
check(recipes and recipes.Alchemy and #recipes.Alchemy.recipes == 3, "recipes enumerated")
check(recipes and recipes.Alchemy and recipes.Alchemy.recipes[1].spellID == 2330,
    "recipe id retained")
check(recipes and recipes.Alchemy and recipes.Alchemy.capturedAt == world.now,
    "recipes are a cache and must be timestamped")

-- Difficulty must come out as the same word the Classic path produces. The
-- client reports an enum integer here; storing it raw put a number where every
-- reader expects a word and made the Rust side reject the whole file — every
-- character, every bag, over one recipe field.
local by_id = {}
for _, r in ipairs(recipes and recipes.Alchemy and recipes.Alchemy.recipes or {}) do
    by_id[r.spellID] = r
end
check(by_id[2330] and by_id[2330].difficulty == "optimal",
    "enum 0 must be normalised to the word the Classic path uses")
check(by_id[2337] and by_id[2337].difficulty == "trivial", "enum 3 is trivial")
check(by_id[2330] and type(by_id[2330].difficulty) == "string",
    "difficulty is a string in this schema whatever the client returns")
-- A value this build has never heard of must drop out, not be guessed at.
-- Absent already means "not captured" everywhere here, and the reader treats
-- an unrated recipe as one that might still raise the skill.
check(by_id[9999] ~= nil, "an unknown difficulty must not drop the recipe itself")
check(by_id[9999] and by_id[9999].difficulty == nil,
    "an unrecognised enum value becomes absent rather than a guess")

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

-- Per-node detail is unproven on the live client. If it throws, the tree
-- totals must survive without it.
local realGetNodeInfo = C_Traits.GetNodeInfo
C_Traits.GetNodeInfo = function() error("node API changed") end
ok, err = pcall(collector.capture)
check(ok, "capture must survive a failing node API: " .. tostring(err))
local survived = record().talents
check(survived and #survived.trees == 3, "tree totals kept when node detail fails")
check(survived and survived.trees[3].talents == nil, "no partial detail stored when node detail fails")
C_Traits.GetNodeInfo = realGetNodeInfo
collector.capture()
check(record().talents.trees[3].talents ~= nil, "detail returns once the node API works again")

--------------------------------------------------------------------------------

if failures > 0 then
    io.stderr:write(string.format("%d assertion(s) failed\n", failures))
    os.exit(1)
end
print("forever collector tests passed")

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
    -- WoW emits array entries as bare values, not as [1] = ..., so the
    -- fixtures must look the same or they test a format nothing produces.
    for _, key in ipairs(numeric) do
        parts[#parts + 1] = inner .. serialize(value[key], indent + 1) .. ","
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
