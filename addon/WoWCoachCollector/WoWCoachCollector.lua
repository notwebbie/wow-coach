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

local ADDON_NAME = ...
local SCHEMA_VERSION = 2
local THROTTLE_SECONDS = 5

local collector = {}

--------------------------------------------------------------------------------
-- API shims
--
-- Container globals were removed from the 2.5.x-lineage client in favour of
-- C_Container, and GetItemInfo/GetTalentTabInfo now exist only as deprecation
-- shims the user can disable. Every call below prefers the current API and
-- degrades to nil rather than erroring, so one missing API costs one field.
--------------------------------------------------------------------------------

local function containerNumSlots(bag)
    if C_Container and C_Container.GetContainerNumSlots then
        return C_Container.GetContainerNumSlots(bag)
    end
    if GetContainerNumSlots then return GetContainerNumSlots(bag) end
    return nil
end

local function containerNumFreeSlots(bag)
    if C_Container and C_Container.GetContainerNumFreeSlots then
        return C_Container.GetContainerNumFreeSlots(bag)
    end
    if GetContainerNumFreeSlots then return GetContainerNumFreeSlots(bag) end
    return nil
end

local function containerItem(bag, slot)
    if C_Container and C_Container.GetContainerItemInfo then
        local info = C_Container.GetContainerItemInfo(bag, slot)
        if info then return info.itemID, info.stackCount end
        return nil
    end
    if C_Container and C_Container.GetContainerItemID then
        return C_Container.GetContainerItemID(bag, slot), nil
    end
    if GetContainerItemInfo then
        local _, count, _, _, _, _, _, _, _, itemID = GetContainerItemInfo(bag, slot)
        return itemID, count
    end
    return nil
end

local function bagItemID(bag)
    -- Bag 0 is the backpack and has no item; bags 1-4 map to inventory slots.
    if bag == 0 then return nil end
    local inventorySlot
    if C_Container and C_Container.ContainerIDToInventoryID then
        inventorySlot = C_Container.ContainerIDToInventoryID(bag)
    elseif ContainerIDToInventoryID then
        inventorySlot = ContainerIDToInventoryID(bag)
    end
    if not inventorySlot or not GetInventoryItemID then return nil end
    return GetInventoryItemID("player", inventorySlot)
end

--------------------------------------------------------------------------------
-- Flavor
--
-- WOW_PROJECT_ID is no longer sufficient: the WoW Forever client reports
-- WOW_PROJECT_MAINLINE despite being vanilla-era content. The raw interface
-- number is always stored so the reader can reinterpret a flavor it does not
-- yet know about without needing an addon update.
--------------------------------------------------------------------------------

local function interfaceVersion()
    if not GetBuildInfo then return nil end
    local _, _, _, interface = GetBuildInfo()
    return interface
end

local function gameFlavor(interface)
    if interface then
        if interface >= 20000 and interface < 30000 then return "tbc_classic" end
        if interface >= 16000 and interface < 17000 then return "forever" end
        if interface >= 11000 and interface < 16000 then return "classic_era" end
        if interface >= 100000 then return "retail" end
    end
    if WOW_PROJECT_ID then
        if WOW_PROJECT_CLASSIC and WOW_PROJECT_ID == WOW_PROJECT_CLASSIC then return "classic_era" end
        if WOW_PROJECT_BURNING_CRUSADE_CLASSIC and WOW_PROJECT_ID == WOW_PROJECT_BURNING_CRUSADE_CLASSIC then
            return "tbc_classic"
        end
        if WOW_PROJECT_MAINLINE and WOW_PROJECT_ID == WOW_PROJECT_MAINLINE then return "retail" end
    end
    return "unknown"
end

local function characterKey(flavor, realm, name)
    return string.format("%d:%s|%d:%s|%d:%s", #flavor, flavor, #realm, realm, #name, name)
end

--------------------------------------------------------------------------------
-- Capture helpers
--------------------------------------------------------------------------------

local function captureSkills()
    if not GetNumSkillLines or not GetSkillLineInfo then return nil, nil end
    local skills, complete = {}, true
    for index = 1, (GetNumSkillLines() or 0) do
        -- Returns: name, isHeader, isExpanded, rank, numTempPoints, modifier, maxRank, ...
        -- Rank is position 4 and maxRank position 7. numTempPoints is deliberately
        -- NOT added: Blizzard adds it for display, but it is a temporary buff.
        local name, isHeader, isExpanded, rank, _, _, maxRank = GetSkillLineInfo(index)
        if isHeader then
            if isExpanded == false then complete = false end
        elseif name and rank and rank > 0 then
            skills[#skills + 1] = { name = name, rank = rank, maxRank = maxRank }
        end
    end
    return skills, complete
end

local function captureTalents()
    if not GetNumTalentTabs then return nil end
    local numTabs = GetNumTalentTabs(false, false)
    if not numTabs or numTabs == 0 then return nil end

    local trees = {}
    for index = 1, numTabs do
        local name, pointsSpent
        if C_SpecializationInfo and C_SpecializationInfo.GetSpecializationInfo then
            -- Returns: specId, name, description, icon, role, primaryStat, pointsSpent, ...
            local _, tabName, _, _, _, _, spent =
                C_SpecializationInfo.GetSpecializationInfo(index, false, false, nil, nil, nil)
            name, pointsSpent = tabName, spent
        elseif GetTalentTabInfo then
            -- Deprecation shim: specId, name, description, icon, pointsSpent, ...
            local _, tabName, _, _, spent = GetTalentTabInfo(index, false, false)
            name, pointsSpent = tabName, spent
        end
        if name then
            trees[#trees + 1] = { name = name, pointsSpent = pointsSpent or 0 }
        end
    end
    if #trees == 0 then return nil end

    local unspent
    if GetUnspentTalentPoints then unspent = GetUnspentTalentPoints(false, false, nil) end
    return { trees = trees, unspentPoints = unspent }
end

local function captureQuests()
    if not GetNumQuestLogEntries or not GetQuestLogTitle then return nil, nil end
    local numEntries = GetNumQuestLogEntries()
    if not numEntries then return nil, nil end

    local quests, header, complete = {}, nil, true
    for index = 1, numEntries do
        -- Returns: title, level, questTag, isHeader, isCollapsed, isComplete,
        --          frequency, questID, ...
        local title, level, _, isHeader, isCollapsed, isComplete, _, questID =
            GetQuestLogTitle(index)
        if isHeader then
            header = title
            -- A collapsed header hides its quests from enumeration, so the
            -- capture is partial. Record that rather than expanding the header,
            -- which would change the user's UI behind their back.
            if isCollapsed then complete = false end
        elseif title then
            local state = "active"
            if isComplete then
                if isComplete > 0 then state = "complete"
                elseif isComplete < 0 then state = "failed" end
            end
            quests[#quests + 1] = {
                questID = questID,
                title = title,
                level = level,
                header = header,
                state = state,
            }
        end
    end
    return quests, complete
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
                bagIndex = bag,
                itemID = bagItemID(bag),
                slots = slots,
                freeSlots = free,
            }
            for slot = 1, slots do
                local itemID, count = containerItem(bag, slot)
                if itemID then
                    items[itemID] = (items[itemID] or 0) + (count or 1)
                end
            end
        end
    end

    if not sawAnyBag then return nil end

    -- Emit the item tally as an array of records. A map keyed by item ID would
    -- serialise as ["12345"] and force the reader to parse keys as numbers.
    local contents = {}
    for itemID, count in pairs(items) do
        contents[#contents + 1] = { itemID = itemID, count = count }
    end
    table.sort(contents, function(a, b) return a.itemID < b.itemID end)

    return {
        bags = bags,
        totalSlots = totalSlots,
        freeSlots = freeSlots,
        contents = contents,
    }
end

--------------------------------------------------------------------------------
-- Recipes
--
-- Trade skill and craft data is only readable while the relevant window is
-- open, so it cannot be refreshed at logout like everything else. It is
-- captured opportunistically when the player opens a profession and stored as
-- a timestamped cache. TBC keeps Enchanting on the older Craft API, so both
-- systems are required.
--------------------------------------------------------------------------------

local function spellIDFromLink(link)
    if type(link) ~= "string" then return nil end
    local id = link:match("|Henchant:(%d+)") or link:match("|Hspell:(%d+)")
    return id and tonumber(id) or nil
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
    local flavor = gameFlavor(interface)

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
end

function collector.captureRecipes()
    local record = currentRecord()
    if not record then return end

    local profession, data = captureTradeSkill()
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
    "PLAYER_LOGIN",
    "PLAYER_ENTERING_WORLD",
    "PLAYER_LEVEL_UP",
    "PLAYER_MONEY",
    "PLAYER_XP_UPDATE",
    "PLAYER_UPDATE_RESTING",
    "UPDATE_EXHAUSTION",
    "ZONE_CHANGED_NEW_AREA",
    "QUEST_LOG_UPDATE",
    "BAG_UPDATE_DELAYED",
    "SKILL_LINES_CHANGED",
    "CHARACTER_POINTS_CHANGED",
}

local RECIPE_EVENTS = {
    "TRADE_SKILL_SHOW",
    "TRADE_SKILL_UPDATE",
    "CRAFT_SHOW",
    "CRAFT_UPDATE",
}

for _, event in ipairs(SNAPSHOT_EVENTS) do frame:RegisterEvent(event) end
for _, event in ipairs(RECIPE_EVENTS) do frame:RegisterEvent(event) end

local pending, sinceFlush = false, 0

local function isRecipeEvent(event)
    for _, candidate in ipairs(RECIPE_EVENTS) do
        if candidate == event then return true end
    end
    return false
end

frame:SetScript("OnEvent", function(_, event, unit)
    if isRecipeEvent(event) then
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
