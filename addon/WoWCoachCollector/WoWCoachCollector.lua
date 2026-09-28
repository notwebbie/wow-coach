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

-- The item's name, so a reader has something better to show than a number.
--
-- This is a cache lookup, not a request. `GetItemInfo` returns nothing for an
-- item the client has not loaded yet, and we neither wait for it nor ask the
-- server: a capture that blocks is a capture that sometimes never happens. The
-- ID is the fact and the name is a convenience, so a missing name is recorded
-- as missing, exactly like every other field in this schema.
local function itemName(itemID)
    local name
    if has("C_Item.GetItemInfo") then
        name = C_Item.GetItemInfo(itemID)
    elseif GetItemInfo then
        name = GetItemInfo(itemID)
    end
    if type(name) == "string" and name ~= "" then return name end
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

-- Walk a list of container ids and total up what is in them.
--
-- Shared by the bag and bank captures, because the two differ only in which
-- containers they read and in when reading them is valid. Returns nil when no
-- container answered at all, which is what separates "could not look" from
-- "looked, and it was empty".
local function captureContainers(ids)
    local bags, items = {}, {}
    local totalSlots, freeSlots, sawAnyBag = 0, 0, false

    for _, bag in ipairs(ids) do
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
        contents[#contents + 1] = { itemID = itemID, count = count, name = itemName(itemID) }
    end
    table.sort(contents, function(a, b) return a.itemID < b.itemID end)

    return { bags = bags, totalSlots = totalSlots, freeSlots = freeSlots, contents = contents }
end

local BAG_IDS = { 0, 1, 2, 3, 4 }

-- The bank: the main window (-1), the seven purchasable bag slots (5..11), and
-- the reagent bank (-3) on clients that have one. A container the client does
-- not have reports no slots, so naming one here costs nothing and a client
-- that grows one is captured without an addon update.
local BANK_IDS = { -1, -3, 5, 6, 7, 8, 9, 10, 11 }

local function captureBags()
    return captureContainers(BAG_IDS)
end

-- The bank, which is only readable while its window is open.
--
-- Away from a banker these calls do not fail. They answer emptily, and an
-- empty answer stored as a fact reads as "this character owns nothing" — which
-- would wipe a real holding every time the player logged in somewhere else.
-- That is the same shape as the logout bug: a value read at a moment the
-- client cannot supply it is not a measurement.
--
-- So the caller gates this on the bank frame being open, and the snapshot
-- carries the time it was taken. A bank seen last Tuesday is still the best
-- information anyone has; it simply has to say so rather than pass for current.
local function captureBank()
    local bank = captureContainers(BANK_IDS)
    if not bank then return nil end
    bank.capturedAt = time()
    return bank
end

--------------------------------------------------------------------------------
-- Spells
--
-- The whole spellbook, by id, and deliberately not a curated list.
--
-- The immediate use is travel: knowing which character can reach which city
-- turns "the reagents are on your bank alt" into a route. It would be far
-- cheaper to check a handful of known teleport ids here and store a `teleports`
-- table — and it would put a rule in the collector, which is the one thing this
-- addon is not allowed to do. The list would then need an addon update for
-- every expansion, would be wrong on any client whose ids differ, and would
-- silently answer "no teleports" rather than "I was not looking for that one".
--
-- So the collector reports what the character knows, and the core decides what
-- any of it means. Ids rather than names: an id is the same in every locale,
-- and in Classic each rank is its own id, so the id already carries the rank.
--
-- FUTURESPELL entries are skipped. The spellbook shows spells the character is
-- eligible for but has not trained, greyed out, and recording one would turn
-- "could learn this" into "knows this" — the exact claim a player would act on.
--------------------------------------------------------------------------------

local function captureSpellsModern()
    if not has("C_SpellBook.GetNumSpellBookSkillLines") then return nil end
    if not has("C_SpellBook.GetSpellBookItemInfo") then return nil end
    local spells, seen = {}, {}
    local bank = Enum and Enum.SpellBookSpellBank and Enum.SpellBookSpellBank.Player or 0
    for line = 1, (C_SpellBook.GetNumSpellBookSkillLines() or 0) do
        local info = C_SpellBook.GetSpellBookSkillLineInfo(line)
        if info and info.itemIndexOffset and info.numSpellBookItems then
            for offset = 1, info.numSpellBookItems do
                local item = C_SpellBook.GetSpellBookItemInfo(info.itemIndexOffset + offset, bank)
                -- itemType distinguishes a known spell from a greyed-out one
                -- and from a flyout container. Only the first is a fact about
                -- what this character can cast.
                local known = item and item.spellID and not item.isOffSpec
                    and item.itemType == (Enum.SpellBookItemType and Enum.SpellBookItemType.Spell)
                if known and not seen[item.spellID] then
                    seen[item.spellID] = true
                    spells[#spells + 1] = { spellID = item.spellID, name = item.name }
                end
            end
        end
    end
    if #spells == 0 then return nil end
    return spells
end

local function captureSpellsClassic()
    if not GetNumSpellTabs or not GetSpellTabInfo then return nil end
    if not GetSpellBookItemInfo then return nil end
    local spells, seen = {}, {}
    for tab = 1, (GetNumSpellTabs() or 0) do
        -- name, texture, offset, numSpells
        local _, _, offset, numSpells = GetSpellTabInfo(tab)
        if offset and numSpells then
            for index = offset + 1, offset + numSpells do
                local skillType, spellID = GetSpellBookItemInfo(index, "spell")
                -- "FUTURESPELL" is the greyed-out entry for a spell this
                -- character is high enough for but has not trained.
                if skillType == "SPELL" and spellID and not seen[spellID] then
                    seen[spellID] = true
                    local name
                    if GetSpellBookItemName then name = GetSpellBookItemName(index, "spell") end
                    spells[#spells + 1] = { spellID = spellID, name = name }
                end
            end
        end
    end
    if #spells == 0 then return nil end
    return spells
end

local function captureSpells()
    return captureSpellsModern() or captureSpellsClassic()
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

-- The skill window is a list of headers each followed by its own skills, and
-- the header is the game's own answer to "what kind of skill is this": a
-- profession, a weapon skill, a language, an armour proficiency. Discarding it
-- and trying to work the answer back out — from the name, from whether the
-- rank looks round — means guessing at something the client already said, in a
-- way that breaks in every locale but this one.
--
-- So the header is recorded verbatim against each skill. It is not
-- interpreted here and it must not be matched on; it exists so a reader can
-- group skills the way the player sees them grouped in game.
local function captureSkills()
    if has("C_SkillInfo.GetNumSkillLines") and has("C_SkillInfo.GetSkillLineInfo") then
        local skills, complete, header = {}, true, nil
        for index = 1, (C_SkillInfo.GetNumSkillLines() or 0) do
            local info = C_SkillInfo.GetSkillLineInfo(index)
            if info then
                if info.isHeader then
                    header = info.name
                    if info.isCollapsed then complete = false end
                elseif info.name and info.rank and info.rank > 0 then
                    skills[#skills + 1] = {
                        name = info.name, rank = info.rank, maxRank = info.maxRank,
                        skillID = info.skillID, header = header,
                        -- Forever carries a numeric category, which is the
                        -- same answer without the locale problem. Recorded
                        -- where it exists; absent everywhere else.
                        categoryID = info.skillLineCategoryID,
                    }
                end
            end
        end
        return skills, complete
    end

    if not GetNumSkillLines or not GetSkillLineInfo then return nil, nil end
    local skills, complete, header = {}, true, nil
    for index = 1, (GetNumSkillLines() or 0) do
        -- name, isHeader, isExpanded, rank, tempPoints, modifier, maxRank, ...
        local name, isHeader, isExpanded, rank, _, _, maxRank = GetSkillLineInfo(index)
        if isHeader then
            header = name
            if isExpanded == false then complete = false end
        elseif name and rank and rank > 0 then
            skills[#skills + 1] = {
                name = name, rank = rank, maxRank = maxRank, header = header,
            }
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

local function itemIDFromLink(link)
    if type(link) ~= "string" then return nil end
    local id = link:match("|Hitem:(%d+)")
    return id and tonumber(id) or nil
end

-- What a recipe consumes, as item ids and counts.
--
-- Reagents are the difference between "you know this recipe" and "you can make
-- this": a roster-wide crafting view cannot say where the materials are, or
-- what a craft costs, without them. They are only readable while the trade
-- skill window is open, exactly like the recipe list they belong to, so they
-- are part of the same opportunistic cache and carry the same staleness.
--
-- `playerCount` is deliberately NOT stored. It counts what the crafting
-- character is carrying at that moment, which is a fact about a bag at a
-- timestamp, not about the recipe — and the bag capture already records that
-- properly for every character. Persisting it would put a second, staler
-- answer to the same question into the file.
local function tradeSkillReagents(index)
    if not GetTradeSkillNumReagents then return nil end
    local count = GetTradeSkillNumReagents(index)
    if not count or count == 0 then return nil end
    local reagents = {}
    for slot = 1, count do
        local name, _, required
        if GetTradeSkillReagentInfo then
            name, _, required = GetTradeSkillReagentInfo(index, slot)
        end
        local itemID
        if GetTradeSkillReagentItemLink then
            local ok, link = pcall(GetTradeSkillReagentItemLink, index, slot)
            if ok then itemID = itemIDFromLink(link) end
        end
        if itemID or name then
            reagents[#reagents + 1] = { itemID = itemID, name = name, count = required }
        end
    end
    if #reagents == 0 then return nil end
    return reagents
end

local function craftReagents(index)
    if not GetCraftNumReagents then return nil end
    local count = GetCraftNumReagents(index)
    if not count or count == 0 then return nil end
    local reagents = {}
    for slot = 1, count do
        local name, _, required
        if GetCraftReagentInfo then
            name, _, required = GetCraftReagentInfo(index, slot)
        end
        local itemID
        if GetCraftReagentItemLink then
            local ok, link = pcall(GetCraftReagentItemLink, index, slot)
            if ok then itemID = itemIDFromLink(link) end
        end
        if itemID or name then
            reagents[#reagents + 1] = { itemID = itemID, name = name, count = required }
        end
    end
    if #reagents == 0 then return nil end
    return reagents
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
            local entry = {
                name = recipe.name, spellID = recipeID,
                difficulty = recipe.relativeDifficulty,
            }
            -- The modern client describes a recipe as a schematic rather than
            -- an indexed list, so reagents come from a different shape than
            -- the Classic path. A slot can hold several interchangeable items
            -- (any rank of a herb, say); the first is recorded as the
            -- representative and `choices` says how many others there were, so
            -- a reader can tell a firm requirement from a substitutable one.
            if has("C_TradeSkillUI.GetRecipeSchematic") then
                local ok, schematic = pcall(C_TradeSkillUI.GetRecipeSchematic, recipeID, false)
                if ok and type(schematic) == "table" then
                    entry.makesItemID = schematic.outputItemID
                    entry.makesMin = schematic.quantityMin
                    entry.makesMax = schematic.quantityMax
                    local slots = schematic.reagentSlotSchematics
                    if type(slots) == "table" then
                        local reagents = {}
                        for _, slot in ipairs(slots) do
                            local first = type(slot.reagents) == "table" and slot.reagents[1] or nil
                            if first and first.itemID then
                                reagents[#reagents + 1] = {
                                    itemID = first.itemID,
                                    count = slot.quantityRequired,
                                    choices = #slot.reagents > 1 and #slot.reagents or nil,
                                }
                            end
                        end
                        if #reagents > 0 then entry.reagents = reagents end
                    end
                end
            end
            entries[#entries + 1] = entry
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
            -- What it makes, so the craft can be valued rather than only named.
            -- Absent for a recipe that produces no item — an enchant applied
            -- directly to gear has no output to sell.
            local makesItemID, makesMin, makesMax
            if GetTradeSkillItemLink then
                local ok, link = pcall(GetTradeSkillItemLink, index)
                if ok then makesItemID = itemIDFromLink(link) end
            end
            if GetTradeSkillNumMade then
                local ok, low, high = pcall(GetTradeSkillNumMade, index)
                if ok then makesMin, makesMax = low, high end
            end
            entries[#entries + 1] = {
                name = name, difficulty = difficulty, spellID = spellID,
                reagents = tradeSkillReagents(index),
                makesItemID = makesItemID, makesMin = makesMin, makesMax = makesMax,
            }
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
            -- The Craft API is TBC's Enchanting path. Enchants are applied to
            -- gear rather than produced as items, so there is no output item
            -- to record and none is invented.
            entries[#entries + 1] = {
                name = name, difficulty = difficulty, spellID = spellID,
                reagents = craftReagents(index),
            }
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

    local spells = captureSpells()
    if spells then record.spells = spells end

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

-- Record what is in the bank, and only ever when the window is open.
--
-- Deliberately never clears `record.bank`. The last good look is kept until a
-- better one replaces it, because the alternative — forgetting a mule's
-- contents the moment the player walks away from the banker — makes the
-- feature useless for exactly the character it exists to serve.
function collector.captureBank()
    local record = currentRecord()
    if not record then return end

    local bank = captureBank()
    if not bank then return end
    record.bank = bank
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
    "LEARNED_SPELL_IN_TAB", "SPELLS_CHANGED",
    "GAME_RULES_CHANGED", "UNIT_PET", "UNIT_HAPPINESS",
}

local RECIPE_EVENTS = {
    "TRADE_SKILL_SHOW", "TRADE_SKILL_UPDATE", "TRADE_SKILL_LIST_UPDATE",
    "CRAFT_SHOW", "CRAFT_UPDATE",
}

-- Bank events, handled apart from the snapshot because they are the only
-- window in which the bank can be read at all. BANKFRAME_CLOSED is registered
-- to close that window, not to capture: reading after it would produce the
-- empty answer this whole path exists to avoid.
local BANK_EVENTS = {
    "BANKFRAME_OPENED", "BANKFRAME_CLOSED",
    "PLAYERBANKSLOTS_CHANGED", "PLAYERBANKBAGSLOTS_CHANGED",
    "PLAYERREAGENTBANKSLOTS_CHANGED",
}

local recipeEvent = {}
for _, event in ipairs(RECIPE_EVENTS) do recipeEvent[event] = true end

local bankEvent = {}
for _, event in ipairs(BANK_EVENTS) do bankEvent[event] = true end

-- Registering an event the client does not have raises an error, so each one is
-- attempted individually. A client missing an event loses that trigger, never
-- the addon.
local function register(event)
    if frame.RegisterEvent then pcall(frame.RegisterEvent, frame, event) end
end
for _, event in ipairs(SNAPSHOT_EVENTS) do register(event) end
for _, event in ipairs(RECIPE_EVENTS) do register(event) end
for _, event in ipairs(BANK_EVENTS) do register(event) end

local pending, sinceFlush = false, 0
-- Whether the bank window is open, and so whether the bank can be read.
local bankIsOpen = false

frame:SetScript("OnEvent", function(_, event, unit)
    if recipeEvent[event] then
        collector.captureRecipes()
        return
    end
    if bankEvent[event] then
        if event == "BANKFRAME_CLOSED" then
            bankIsOpen = false
            return
        end
        -- Only the frame opening proves the bank is readable. A slot-changed
        -- event can fire when it is not — moving a stack while the window is
        -- shut, for one — and reading then returns the empty answer that would
        -- erase a real holding. So the flag gates, and the slot events only
        -- refresh a bank already known to be open.
        if event == "BANKFRAME_OPENED" then bankIsOpen = true end
        if bankIsOpen then collector.captureBank() end
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
