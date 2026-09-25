-- WoW Coach Collector
--
-- Captures character state to SavedVariables for the WoW Coach clients to read.
-- Local only: no network calls, no combat automation, no protected actions.
--
-- The schema this writes is a public contract. See docs/COLLECTOR-SCHEMA.md.
-- Values are strings, numbers, booleans and nested tables only, so the reader
-- can parse the file without evaluating Lua.
--
-- This addon captures facts, never judgements. Quest difficulty, rested-XP
-- projection and coaching advice are the core's job; storing a derived value
-- here would freeze a rule into the data.
--
-- TWO CLIENT FAMILIES
--
-- The Classic/TBC clients and the Forever client expose the same character
-- state through entirely different APIs. Dispatch below is by CAPABILITY, not
-- by flavor name: the collector asks which API exists rather than deciding from
-- an interface number, because interface numbers change with every patch while
-- the API surface is what actually matters. A client that grows a new API gets
-- the better path for free.

local ADDON_NAME = ...
local SCHEMA_VERSION = 2
local THROTTLE_SECONDS = 5

local collector = {}

local function has(path)
    local current = _G
    for segment in string.gmatch(path, "[^%.]+") do
        if type(current) ~= "table" then return false end
        current = current[segment]
        if current == nil then return false end
    end
    return true
end

--------------------------------------------------------------------------------
-- Flavor
--
-- WOW_PROJECT_ID cannot identify the flavor: the Forever client reports
-- WOW_PROJECT_MAINLINE despite being vanilla-era content. C_SkillInfo is a
-- namespace that exists on no other client, which makes it the one reliable
-- sentinel. The raw interface number is stored regardless so a reader can
-- classify a flavor this collector predates without an addon update.
--------------------------------------------------------------------------------

local function interfaceVersion()
    if not GetBuildInfo then return nil end
    local _, _, _, interface = GetBuildInfo()
    return interface
end

local function gameFlavor(interface, hasSkillInfoNamespace)
    local mainline = WOW_PROJECT_MAINLINE and WOW_PROJECT_ID == WOW_PROJECT_MAINLINE
    if mainline and hasSkillInfoNamespace then return "forever" end
    if interface then
        if interface >= 20000 and interface < 30000 then return "tbc_classic" end
        if interface >= 16000 and interface < 17000 then return "forever" end
        if interface >= 11000 and interface < 16000 then return "classic_era" end
        if interface >= 100000 then return "retail" end
    end
    if WOW_PROJECT_CLASSIC and WOW_PROJECT_ID == WOW_PROJECT_CLASSIC then return "classic_era" end
    if WOW_PROJECT_BURNING_CRUSADE_CLASSIC and WOW_PROJECT_ID == WOW_PROJECT_BURNING_CRUSADE_CLASSIC then
        return "tbc_classic"
    end
    if mainline then return "retail" end
    return "unknown"
end

local function characterKey(flavor, realm, name)
    return string.format("%d:%s|%d:%s|%d:%s", #flavor, flavor, #realm, realm, #name, name)
end

--------------------------------------------------------------------------------
-- Containers (shared; C_Container on every current client)
--------------------------------------------------------------------------------

local function containerNumSlots(bag)
    if has("C_Container.GetContainerNumSlots") then return C_Container.GetContainerNumSlots(bag) end
    if GetContainerNumSlots then return GetContainerNumSlots(bag) end
    return nil
end

local function containerNumFreeSlots(bag)
    if has("C_Container.GetContainerNumFreeSlots") then return C_Container.GetContainerNumFreeSlots(bag) end
    if GetContainerNumFreeSlots then return GetContainerNumFreeSlots(bag) end
    return nil
end

local function containerItem(bag, slot)
    if has("C_Container.GetContainerItemInfo") then
        local info = C_Container.GetContainerItemInfo(bag, slot)
        if info then return info.itemID, info.stackCount end
        return nil
    end
    if GetContainerItemInfo then
        local _, count, _, _, _, _, _, _, _, itemID = GetContainerItemInfo(bag, slot)
        return itemID, count
    end
    return nil
end

local function bagItemID(bag)
    if bag == 0 then return nil end
    local inventorySlot
    if has("C_Container.ContainerIDToInventoryID") then
        inventorySlot = C_Container.ContainerIDToInventoryID(bag)
    elseif ContainerIDToInventoryID then
        inventorySlot = ContainerIDToInventoryID(bag)
    end
    if not inventorySlot or not GetInventoryItemID then return nil end
    return GetInventoryItemID("player", inventorySlot)
end

local function captureBags()
    local bags, items = {}, {}
    local totalSlots, freeSlots, sawAnyBag = 0, 0, false

    for bag = 0, 4 do
        local slots = containerNumSlots(bag)
        if slots and slots > 0 then
            sawAnyBag = true
            local free = containerNumFreeSlots(bag) or 0
            totalSlots = totalSlots + slots
            freeSlots = freeSlots + free
            bags[#bags + 1] = {
                bagIndex = bag, itemID = bagItemID(bag), slots = slots, freeSlots = free,
            }
            for slot = 1, slots do
                local itemID, count = containerItem(bag, slot)
                if itemID then items[itemID] = (items[itemID] or 0) + (count or 1) end
            end
        end
    end
    if not sawAnyBag then return nil end

    local contents = {}
    for itemID, count in pairs(items) do
        contents[#contents + 1] = { itemID = itemID, count = count }
    end
    table.sort(contents, function(a, b) return a.itemID < b.itemID end)

    return { bags = bags, totalSlots = totalSlots, freeSlots = freeSlots, contents = contents }
end

--------------------------------------------------------------------------------
-- Skills
--
-- Forever re-implements Classic's skill lines under C_SkillInfo, returning a
-- structure instead of a value list. Note isCollapsed is the INVERSE of the
-- Classic client's isExpanded — writing one into the other silently inverts
-- the partial-capture flag on one client.
--
-- On both clients rank is taken raw. The default UI adds temporary points for
-- display, but those are a transient buff and must not be persisted.
--------------------------------------------------------------------------------

local function captureSkills()
    if has("C_SkillInfo.GetNumSkillLines") and has("C_SkillInfo.GetSkillLineInfo") then
        local skills, complete = {}, true
        for index = 1, (C_SkillInfo.GetNumSkillLines() or 0) do
            local info = C_SkillInfo.GetSkillLineInfo(index)
            if info then
                if info.isHeader then
                    if info.isCollapsed then complete = false end
                elseif info.name and info.rank and info.rank > 0 then
                    skills[#skills + 1] = {
                        name = info.name, rank = info.rank, maxRank = info.maxRank,
                        skillID = info.skillID,
                    }
                end
            end
        end
        return skills, complete
    end

    if not GetNumSkillLines or not GetSkillLineInfo then return nil, nil end
    local skills, complete = {}, true
    for index = 1, (GetNumSkillLines() or 0) do
        -- name, isHeader, isExpanded, rank, tempPoints, modifier, maxRank, ...
        local name, isHeader, isExpanded, rank, _, _, maxRank = GetSkillLineInfo(index)
        if isHeader then
            if isExpanded == false then complete = false end
        elseif name and rank and rank > 0 then
            skills[#skills + 1] = { name = name, rank = rank, maxRank = maxRank }
        end
    end
    return skills, complete
end

--------------------------------------------------------------------------------
-- Talents
--
-- Forever renders the vanilla three-tree layout through the retail Trait
-- system: one tree per spec with three node groups standing in for the tabs.
-- The tree comes from the config's own treeIDs — resolving it via the spec
-- index returns nothing, which cost a beta run to discover.
--------------------------------------------------------------------------------

local function captureTalentsFromTraits()
    if not has("C_SpecializationInfo.GetCombatConfigIDForSpecGroup") then return nil end
    if not has("C_Traits.GetGroupDisplayInfoByTreeID") or not has("C_Traits.GetGroupCurrencyInfo") then
        return nil
    end
    local configID = C_SpecializationInfo.GetCombatConfigIDForSpecGroup(1)
    if not configID then return nil end

    local treeID
    if has("C_Traits.GetConfigInfo") then
        local configInfo = C_Traits.GetConfigInfo(configID)
        if configInfo and configInfo.treeIDs then treeID = configInfo.treeIDs[1] end
    end
    if not treeID then return nil end

    local displays = C_Traits.GetGroupDisplayInfoByTreeID(treeID)
    if not displays or #displays == 0 then return nil end

    local groupIDs, names = {}, {}
    for index, display in ipairs(displays) do
        groupIDs[index] = display.groupID
        names[index] = display.displayName
    end
    local currencies = C_Traits.GetGroupCurrencyInfo(configID, groupIDs)
    if not currencies then return nil end

    local trees = {}
    for index, group in ipairs(currencies) do
        local first = group.currencyInfos and group.currencyInfos[1]
        if names[index] then
            trees[#trees + 1] = { name = names[index], pointsSpent = (first and first.spent) or 0 }
        end
    end
    if #trees == 0 then return nil end

    local unspent
    if has("C_Traits.GetTreeCurrencyInfo") then
        local info = C_Traits.GetTreeCurrencyInfo(configID, treeID, false)
        if info and info[1] then unspent = info[1].quantity end
    end
    return { trees = trees, unspentPoints = unspent }
end

local function captureTalentsFromTabs()
    if not GetNumTalentTabs then return nil end
    local numTabs = GetNumTalentTabs(false, false)
    if not numTabs or numTabs == 0 then return nil end

    local trees = {}
    for index = 1, numTabs do
        local name, pointsSpent
        if has("C_SpecializationInfo.GetSpecializationInfo") then
            -- specId, name, description, icon, role, primaryStat, pointsSpent, ...
            local _, tabName, _, _, _, _, spent =
                C_SpecializationInfo.GetSpecializationInfo(index, false, false, nil, nil, nil)
            name, pointsSpent = tabName, spent
        elseif GetTalentTabInfo then
            -- Deprecation shim: specId, name, description, icon, pointsSpent, ...
            local _, tabName, _, _, spent = GetTalentTabInfo(index, false, false)
            name, pointsSpent = tabName, spent
        end
        if name then trees[#trees + 1] = { name = name, pointsSpent = pointsSpent or 0 } end
    end
    if #trees == 0 then return nil end

    local unspent
    if GetUnspentTalentPoints then unspent = GetUnspentTalentPoints(false, false, nil) end
    return { trees = trees, unspentPoints = unspent }
end

local function captureTalents()
    return captureTalentsFromTraits() or captureTalentsFromTabs()
end

--------------------------------------------------------------------------------
-- Quests
--
-- Forever's C_QuestLog keys completion by questID, where the Classic client
-- returns it inline as a SIGNED NUMBER from an index-based call. Mixing the two
-- up is the easiest cross-client bug available, so the two paths are kept
-- entirely separate rather than share a loop.
--------------------------------------------------------------------------------

local function captureQuestsFromQuestLog()
    if not has("C_QuestLog.GetNumQuestLogEntries") or not has("C_QuestLog.GetInfo") then
        return nil, nil
    end
    local shown = C_QuestLog.GetNumQuestLogEntries()
    if not shown then return nil, nil end

    local quests, header, complete = {}, nil, true
    for index = 1, shown do
        local info = C_QuestLog.GetInfo(index)
        if info then
            if info.isHeader then
                header = info.title
                if info.isCollapsed then complete = false end
            elseif info.title then
                local state = "active"
                if info.questID then
                    if has("C_QuestLog.IsComplete") and C_QuestLog.IsComplete(info.questID) then
                        state = "complete"
                    elseif has("C_QuestLog.IsFailed") and C_QuestLog.IsFailed(info.questID) then
                        state = "failed"
                    end
                end
                quests[#quests + 1] = {
                    questID = info.questID, title = info.title, level = info.level,
                    header = header, state = state,
                }
            end
        end
    end
    return quests, complete
end

local function captureQuestsFromTitles()
    if not GetNumQuestLogEntries or not GetQuestLogTitle then return nil, nil end
    local numEntries = GetNumQuestLogEntries()
    if not numEntries then return nil, nil end

    local quests, header, complete = {}, nil, true
    for index = 1, numEntries do
        -- title, level, questTag, isHeader, isCollapsed, isComplete, frequency, questID
        local title, level, _, isHeader, isCollapsed, isComplete, _, questID =
            GetQuestLogTitle(index)
        if isHeader then
            header = title
            if isCollapsed then complete = false end
        elseif title then
            local state = "active"
            if isComplete then
                if isComplete > 0 then state = "complete"
                elseif isComplete < 0 then state = "failed" end
            end
            quests[#quests + 1] = {
                questID = questID, title = title, level = level, header = header, state = state,
            }
        end
    end
    return quests, complete
end

local function captureQuests()
    local quests, complete = captureQuestsFromQuestLog()
    if quests then return quests, complete end
    return captureQuestsFromTitles()
end

--------------------------------------------------------------------------------
-- Ruleset (Forever)
--
-- Forever replaces realms with rulesets. Normal is the absence of all three
-- flags rather than a value of its own, so all three are recorded explicitly:
-- "false" is data, and an absent table means the client has no rulesets.
--------------------------------------------------------------------------------

local function captureRuleset()
    if not has("C_GameRules.IsGameRuleActive") or not Enum or not Enum.GameRule then return nil end
    local rules = { hardcore = "HardcoreRuleset", pvp = "PvPRuleset",
                    rp = "RPRuleset", selfFound = "SelfFoundAllowed" }
    local ruleset, sawAny = {}, false
    for field, member in pairs(rules) do
        local value = Enum.GameRule[member]
        if value ~= nil then
            ruleset[field] = C_GameRules.IsGameRuleActive(value) and true or false
            sawAny = true
        end
    end
    if not sawAny then return nil end
    return ruleset
end

--------------------------------------------------------------------------------
-- Hunter pet
--
-- Happiness, loyalty and training points exist on both client families — under
-- C_PetInfo on Forever and as bare globals on Classic. These functions return
-- NO values (not nil) when no pet is out, so each result is captured into a
-- local before it is used.
--------------------------------------------------------------------------------

local function capturePet()
    if UnitExists and not UnitExists("pet") then return nil end

    local happiness, loyalty, spent, total
    if has("C_PetInfo.GetPetHappiness") then
        happiness = C_PetInfo.GetPetHappiness()
    elseif GetPetHappiness then
        happiness = GetPetHappiness()
    end
    if has("C_PetInfo.GetPetLoyalty") then
        loyalty = C_PetInfo.GetPetLoyalty()
    end
    if has("C_PetInfo.GetPetTrainingPoints") then
        spent, total = C_PetInfo.GetPetTrainingPoints()
    elseif GetPetTrainingPoints then
        spent, total = GetPetTrainingPoints()
    end

    local name = UnitName and UnitName("pet") or nil
    if name == nil and happiness == nil and spent == nil then return nil end
    return {
        name = name, happiness = happiness, loyalty = loyalty,
        trainingPointsSpent = spent, trainingPointsTotal = total,
    }
end

--------------------------------------------------------------------------------
-- Recipes
--
-- Only readable while the profession window is open on the Classic client, and
-- the same guard exists on Forever, so on both this is an opportunistic
-- timestamped cache rather than something refreshable on demand. The Burning
-- Crusade client also keeps Enchanting on a separate older Craft API.
--------------------------------------------------------------------------------

local function spellIDFromLink(link)
    if type(link) ~= "string" then return nil end
    local id = link:match("|Henchant:(%d+)") or link:match("|Hspell:(%d+)")
    return id and tonumber(id) or nil
end

local function captureTradeSkillUI()
    if not has("C_TradeSkillUI.GetBaseProfessionInfo") then return nil end
    if has("C_TradeSkillUI.IsTradeSkillReady") and not C_TradeSkillUI.IsTradeSkillReady() then
        return nil
    end
    local info = C_TradeSkillUI.GetBaseProfessionInfo()
    if not info or not info.professionName or info.professionName == "" then return nil end
    if not has("C_TradeSkillUI.GetAllRecipeIDs") or not has("C_TradeSkillUI.GetRecipeInfo") then
        return nil
    end
    local ids = C_TradeSkillUI.GetAllRecipeIDs()
    if type(ids) ~= "table" or #ids == 0 then return nil end

    local entries = {}
    for _, recipeID in ipairs(ids) do
        local recipe = C_TradeSkillUI.GetRecipeInfo(recipeID)
        if type(recipe) == "table" and recipe.name then
            entries[#entries + 1] = {
                name = recipe.name, spellID = recipeID,
                difficulty = recipe.relativeDifficulty,
            }
        end
    end
    if #entries == 0 then return nil end
    return info.professionName,
        { rank = info.skillLevel, maxRank = info.maxSkillLevel, recipes = entries }
end

local function captureTradeSkill()
    if not GetTradeSkillLine or not GetNumTradeSkills or not GetTradeSkillInfo then return nil end
    local profession, rank, maxRank = GetTradeSkillLine()
    if not profession or profession == "" or profession == "UNKNOWN" then return nil end

    local entries = {}
    for index = 1, (GetNumTradeSkills() or 0) do
        local name, difficulty = GetTradeSkillInfo(index)
        if name and difficulty and difficulty ~= "header" then
            local spellID
            if GetTradeSkillRecipeLink then
                local ok, link = pcall(GetTradeSkillRecipeLink, index)
                if ok then spellID = spellIDFromLink(link) end
            end
            entries[#entries + 1] = { name = name, difficulty = difficulty, spellID = spellID }
        end
    end
    if #entries == 0 then return nil end
    return profession, { rank = rank, maxRank = maxRank, recipes = entries }
end

local function captureCraft()
    if not GetCraftDisplaySkillLine or not GetNumCrafts or not GetCraftInfo then return nil end
    local profession, rank, maxRank = GetCraftDisplaySkillLine()
    if not profession or profession == "" then return nil end

    local entries = {}
    for index = 1, (GetNumCrafts() or 0) do
        local name, _, difficulty = GetCraftInfo(index)
        if name and difficulty and difficulty ~= "header" then
            local spellID
            if GetCraftRecipeLink then
                local ok, link = pcall(GetCraftRecipeLink, index)
                if ok then spellID = spellIDFromLink(link) end
            end
            entries[#entries + 1] = { name = name, difficulty = difficulty, spellID = spellID }
        end
    end
    if #entries == 0 then return nil end
    return profession, { rank = rank, maxRank = maxRank, recipes = entries }
end

--------------------------------------------------------------------------------
-- Snapshot
--------------------------------------------------------------------------------

local function currentRecord()
    local name = UnitName and UnitName("player")
    local realm = GetRealmName and GetRealmName()
    if not name or name == "" or not realm or realm == "" then return nil end

    local interface = interfaceVersion()
    local flavor = gameFlavor(interface, has("C_SkillInfo"))

    WoWCoachCollectorDB = WoWCoachCollectorDB or {}
    WoWCoachCollectorDB.schemaVersion = SCHEMA_VERSION
    WoWCoachCollectorDB.characters = WoWCoachCollectorDB.characters or {}

    local key = characterKey(flavor, realm, name)
    local record = WoWCoachCollectorDB.characters[key]
    if not record then
        record = {}
        WoWCoachCollectorDB.characters[key] = record
    end
    return record, flavor, interface, realm, name
end

function collector.capture()
    local record, flavor, interface, realm, name = currentRecord()
    if not record then return end

    local class, classID, race, raceID
    if UnitClass then
        local _, classFile, id = UnitClass("player")
        class, classID = classFile, id
    end
    if UnitRace then
        local _, raceFile, id = UnitRace("player")
        race, raceID = raceFile, id
    end

    record.schemaVersion = SCHEMA_VERSION
    record.capturedAt = time()
    record.gameFlavor = flavor
    record.interfaceVersion = interface
    record.realm = realm
    record.name = name
    record.level = UnitLevel and UnitLevel("player")
    record.class = class
    record.classID = classID
    record.race = race
    record.raceID = raceID
    record.faction = UnitFactionGroup and UnitFactionGroup("player")
    record.zone = GetRealZoneText and GetRealZoneText()
    record.subZone = GetSubZoneText and GetSubZoneText()
    record.bindLocation = GetBindLocation and GetBindLocation()
    if IsResting then record.isResting = IsResting() and true or false end
    record.moneyCopper = GetMoney and GetMoney()
    record.xp = UnitXP and UnitXP("player")
    record.maxXP = UnitXPMax and UnitXPMax("player")
    -- Returns nil when the character has no rested bonus. Stored as 0 in that
    -- case so an absent field always means "not captured", never "not rested".
    if GetXPExhaustion then record.restedXP = GetXPExhaustion() or 0 end

    local skills, skillsComplete = captureSkills()
    if skills then
        record.skills = skills
        record.skillsComplete = skillsComplete
    end

    local talents = captureTalents()
    if talents then record.talents = talents end

    local quests, questsComplete = captureQuests()
    if quests then
        record.quests = quests
        record.questsComplete = questsComplete
    end

    local inventory = captureBags()
    if inventory then record.inventory = inventory end

    local ruleset = captureRuleset()
    if ruleset then record.ruleset = ruleset end

    local pet = capturePet()
    if pet then record.pet = pet end
end

function collector.captureRecipes()
    local record = currentRecord()
    if not record then return end

    local profession, data = captureTradeSkillUI()
    if not profession then profession, data = captureTradeSkill() end
    if not profession then profession, data = captureCraft() end
    if not profession or not data then return end

    record.recipes = record.recipes or {}
    data.capturedAt = time()
    record.recipes[profession] = data
end

--------------------------------------------------------------------------------
-- Events
--
-- Bag, quest and skill events fire in bursts, so captures are coalesced behind
-- a dirty flag and flushed on a timer rather than run per event.
--------------------------------------------------------------------------------

local frame = CreateFrame("Frame")

local SNAPSHOT_EVENTS = {
    "PLAYER_LOGIN", "PLAYER_ENTERING_WORLD", "PLAYER_LEVEL_UP", "PLAYER_MONEY",
    "PLAYER_XP_UPDATE", "PLAYER_UPDATE_RESTING", "UPDATE_EXHAUSTION",
    "ZONE_CHANGED_NEW_AREA", "QUEST_LOG_UPDATE", "BAG_UPDATE_DELAYED",
    "SKILL_LINES_CHANGED", "CHARACTER_POINTS_CHANGED", "TRAIT_CONFIG_UPDATED",
    "GAME_RULES_CHANGED", "UNIT_PET", "UNIT_HAPPINESS",
}

local RECIPE_EVENTS = {
    "TRADE_SKILL_SHOW", "TRADE_SKILL_UPDATE", "TRADE_SKILL_LIST_UPDATE",
    "CRAFT_SHOW", "CRAFT_UPDATE",
}

local recipeEvent = {}
for _, event in ipairs(RECIPE_EVENTS) do recipeEvent[event] = true end

-- Registering an event the client does not have raises an error, so each one is
-- attempted individually. A client missing an event loses that trigger, never
-- the addon.
local function register(event)
    if frame.RegisterEvent then pcall(frame.RegisterEvent, frame, event) end
end
for _, event in ipairs(SNAPSHOT_EVENTS) do register(event) end
for _, event in ipairs(RECIPE_EVENTS) do register(event) end

local pending, sinceFlush = false, 0

frame:SetScript("OnEvent", function(_, event, unit)
    if recipeEvent[event] then
        collector.captureRecipes()
        return
    end
    -- PLAYER_XP_UPDATE carries the unit that changed; ignore everyone else.
    if event == "PLAYER_XP_UPDATE" and unit ~= "player" then return end
    if event == "PLAYER_LOGIN" or event == "PLAYER_ENTERING_WORLD" then
        collector.capture()
        pending, sinceFlush = false, 0
        return
    end
    pending = true
end)

if frame.SetScript then
    frame:SetScript("OnUpdate", function(_, elapsed)
        if not pending then return end
        sinceFlush = sinceFlush + (elapsed or 0)
        if sinceFlush < THROTTLE_SECONDS then return end
        sinceFlush = 0
        pending = false
        collector.capture()
    end)
end

collector.SCHEMA_VERSION = SCHEMA_VERSION
collector.frame = frame
collector.gameFlavor = gameFlavor
collector.characterKey = characterKey
_G[ADDON_NAME] = collector
