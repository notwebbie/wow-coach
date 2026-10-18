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
-- Professions
--
-- `captureSkills` above lists every skill line, which includes languages,
-- weapon proficiencies and armour types alongside the trade skills, and gives
-- no way to tell them apart that is not a name in the player's own language.
-- `GetProfessions` does: it returns only the skills the game itself files as
-- professions. That is the second, trusted signal the reader needs, and
-- without it a gathering skill with no trade skill window — Skinning on
-- Burning Crusade, say — was indistinguishable from Orcish.
--
-- TWO THINGS DELIBERATELY NOT ASSUMED
--
-- **The position of a return is not read as a meaning.** The documented
-- signature is prof1, prof2, archaeology, fishing, cooking, firstAid, and a
-- live Forever capture returned 6,7,5,nil,8,nil where slot 3 resolved to First
-- Aid — so either the order differs on this client family or the documentation
-- does, and nothing here needs to know which. Every returned index is passed
-- straight to `GetProfessionInfo`, which names the profession itself. The slot
-- is recorded as a fact in case the ordering is ever worth learning; it is
-- never interpreted.
--
-- **Nils in the middle are not a terminator.** The returns are sparse by
-- design, and `ipairs` over `{ GetProfessions() }` stops at the first hole —
-- which is exactly how the probe that found this API under-reported a
-- character's Cooking. The arity comes from `select("#", ...)` and every slot
-- up to it is read.
--
-- **An empty list is not an answer.** It was documented as one -- "the client
-- was asked and said none" -- and live data killed that within a week. On the
-- Anniversary client (interface 20506) GetProfessions exists, so the capability
-- check above passes, and it returns nothing at all: two characters carrying
-- Tailoring, Enchanting, Mining and Skinning between them both came back empty.
-- It is a stub that answers. So `professionsReturned` is recorded beside the
-- list, and readers decide from that rather than from the list being empty.
--------------------------------------------------------------------------------

local function professionSlots(...)
    local count = select("#", ...)
    local slots = {}
    for slot = 1, count do
        local index = (select(slot, ...))
        if type(index) == "number" then
            local name, _, rank, maxRank, _, _, skillLine = GetProfessionInfo(index)
            if name then
                slots[#slots + 1] = {
                    slot = slot,
                    index = index,
                    name = name,
                    rank = rank,
                    maxRank = maxRank,
                    skillLine = skillLine,
                }
            end
        end
    end
    return slots, count
end

local function captureProfessions()
    if type(GetProfessions) ~= "function" then return nil end
    if type(GetProfessionInfo) ~= "function" then return nil end
    return professionSlots(GetProfessions())
end

--------------------------------------------------------------------------------
-- Talents
--
-- Forever renders the vanilla three-tree layout through the retail Trait
-- system: one tree per spec with three node groups standing in for the tabs.
-- The tree comes from the config's own treeIDs — resolving it via the spec
-- index returns nothing, which cost a beta run to discover.
--------------------------------------------------------------------------------

-- Per-talent detail. Each talent is stored as the game lays it out: a name,
-- its tier (row) and column within its tree, the rank bought and the most it
-- can take, and the talent it hangs off when there is an arrow into it. The
-- planner needs all four, and nothing here is a recommendation.

local function spellName(spellID)
    if not spellID then return nil end
    if has("C_Spell.GetSpellName") then
        local name = C_Spell.GetSpellName(spellID)
        if name then return name end
    end
    if GetSpellInfo then
        local first = GetSpellInfo(spellID)
        if type(first) == "table" then return first.name end
        return first
    end
    return nil
end

-- Row and column numbers from pixel positions. Forever exposes the vanilla
-- grid only as node coordinates, and the live client does not put a row's
-- nodes at exactly the same height: on the first real capture, ranking every
-- distinct value turned seven rows into nine and moved talents a tier down.
-- So positions are grouped: a gap more than a quarter of the largest gap
-- between neighbours starts a new row (or column). Rows are evenly spaced, so
-- the largest gap is at least one row, and placement jitter is far smaller.
local function ordinals(values)
    local distinct, seen = {}, {}
    for _, value in ipairs(values) do
        if not seen[value] then
            seen[value] = true
            distinct[#distinct + 1] = value
        end
    end
    table.sort(distinct)
    local largest = 0
    for i = 2, #distinct do
        largest = math.max(largest, distinct[i] - distinct[i - 1])
    end
    local index, position = {}, 0
    for i, value in ipairs(distinct) do
        if i == 1 or distinct[i] - distinct[i - 1] > largest / 4 then position = position + 1 end
        index[value] = position
    end
    return index
end

local function traitNodeTalents(configID, treeID, groups)
    if not has("C_Traits.GetTreeNodes") or not has("C_Traits.GetNodeInfo") then return nil end
    local nodeIDs = C_Traits.GetTreeNodes(treeID)
    if not nodeIDs or #nodeIDs == 0 then return nil end

    local wanted = {}
    for _, groupID in ipairs(groups) do wanted[groupID] = true end

    local byGroup, nodes, edges = {}, {}, {}
    for _, nodeID in ipairs(nodeIDs) do
        local info = C_Traits.GetNodeInfo(configID, nodeID)
        local group
        if info and info.ID and info.ID ~= 0 and info.groupIDs then
            for _, groupID in ipairs(info.groupIDs) do
                if wanted[groupID] then group = groupID break end
            end
        end
        if group then
            local entryID = (info.activeEntry and info.activeEntry.entryID)
                or (info.entryIDs and info.entryIDs[1])
            local spellID, name
            if entryID and has("C_Traits.GetEntryInfo") and has("C_Traits.GetDefinitionInfo") then
                local entry = C_Traits.GetEntryInfo(configID, entryID)
                local definition = entry and entry.definitionID
                    and C_Traits.GetDefinitionInfo(entry.definitionID)
                if definition then
                    spellID = definition.spellID
                    name = definition.overrideName
                    if name == "" then name = nil end
                end
            end
            name = name or spellName(spellID)
            if name then
                local talent = {
                    name = name,
                    spellID = spellID,
                    -- Committed ranks only: currentRank also counts points
                    -- placed in the frame but not yet applied.
                    rank = info.ranksPurchased or info.activeRank or info.currentRank or 0,
                    maxRank = info.maxRanks,
                }
                nodes[nodeID] = { talent = talent, group = group, x = info.posX, y = info.posY }
                byGroup[group] = byGroup[group] or {}
                table.insert(byGroup[group], nodeID)
                for _, edge in ipairs(info.visibleEdges or {}) do
                    if edge.targetNode then edges[#edges + 1] = { from = nodeID, to = edge.targetNode } end
                end
            end
        end
    end

    local result = {}
    for _, groupID in ipairs(groups) do
        local ids = byGroup[groupID] or {}
        local xs, ys = {}, {}
        for _, nodeID in ipairs(ids) do
            local node = nodes[nodeID]
            if node.x and node.y then
                xs[#xs + 1] = node.x
                ys[#ys + 1] = node.y
            end
        end
        local column, tier = ordinals(xs), ordinals(ys)
        for _, nodeID in ipairs(ids) do
            local node = nodes[nodeID]
            if node.x and node.y then
                node.talent.tier = tier[node.y]
                node.talent.column = column[node.x]
                -- The raw coordinates too: if the grouping is ever wrong,
                -- the core can redo it without an addon update.
                node.talent.posX = node.x
                node.talent.posY = node.y
            end
        end
    end
    for _, edge in ipairs(edges) do
        local from, to = nodes[edge.from], nodes[edge.to]
        if from and to and from.group == to.group and from.talent.tier and from.talent.column then
            to.talent.requires = { tier = from.talent.tier, column = from.talent.column }
        end
    end

    for _, groupID in ipairs(groups) do
        local list = {}
        for _, nodeID in ipairs(byGroup[groupID] or {}) do list[#list + 1] = nodes[nodeID].talent end
        table.sort(list, function(a, b)
            if (a.tier or 0) ~= (b.tier or 0) then return (a.tier or 0) < (b.tier or 0) end
            if (a.column or 0) ~= (b.column or 0) then return (a.column or 0) < (b.column or 0) end
            return a.name < b.name
        end)
        result[groupID] = list
    end
    return result
end

local function tabTalents(tab)
    if not GetNumTalents or not GetTalentInfo then return nil end
    local count = GetNumTalents(tab, false, false)
    if not count or count == 0 then return nil end
    local list = {}
    for index = 1, count do
        local name, _, tier, column, rank, maxRank = GetTalentInfo(tab, index, false, false)
        if type(name) == "table" then
            local info = name
            name, tier, column = info.name, info.tier, info.column
            rank, maxRank = info.rank, info.maxRank
        end
        if name and tier and column then
            local talent = { name = name, tier = tier, column = column, rank = rank or 0, maxRank = maxRank }
            if GetTalentPrereqs then
                local requiredTier, requiredColumn = GetTalentPrereqs(tab, index, false, false)
                if requiredTier and requiredColumn then
                    talent.requires = { tier = requiredTier, column = requiredColumn }
                end
            end
            list[#list + 1] = talent
        end
    end
    if #list == 0 then return nil end
    return list
end

--------------------------------------------------------------------------------
-- Legacy
--
-- WoW Forever's account-wide perk trees. The one that matters to the coaching
-- engine is "Well Rested" in the Adventure tree, because it changes both the
-- rested fill rate and the rested cap -- the two numbers the play-next ranking
-- is built on. Both were measured on a character with no Legacy points, so a
-- player who has bought the perk is being ranked against a baseline that does
-- not apply to them, and until now nothing in the capture said so.
--
-- **Ranks purchased is the fact that settles it, and it needs no names.** If
-- every node in the tree reads zero, nothing is bought and the baseline is
-- exactly right. Identifying which node is Well Rested would be nice and is
-- not required for that, so a nil name costs nothing here.
--
-- Names are captured anyway, through the same spell fallback the talent reader
-- uses. The probe that first explored these trees read only
-- `definition.overrideName` -- which is set only when a trait deliberately
-- overrides its spell's name -- and came back with nil for all nine nodes,
-- making the tree look less readable than it is.
--
-- An absent `configID` means the client would not hand one over, which is a
-- different thing from a tree with nothing bought in it. It is recorded as
-- absent rather than as an empty list, for the same reason professions are.
--------------------------------------------------------------------------------

local LEGACY_TREES = {
    { key = "professions", treeID = 1187 },
    { key = "adventure", treeID = 1188 },
    { key = "resourcefulness", treeID = 1189 },
}

local legacyNodes

local function legacyTree(entry)
    local tree = { tree = entry.key, treeID = entry.treeID }
    local configID = C_Traits.GetConfigIDByTreeID(entry.treeID)
    if not configID then return tree end
    tree.configID = configID

    if has("C_Traits.GetTreeCurrencyInfo") then
        local info = C_Traits.GetTreeCurrencyInfo(configID, entry.treeID, true)
        local first = info and info[1]
        if first then
            tree.available = first.quantity
            tree.cap = first.maxQuantity
            tree.spent = first.spentInTree
        end
    end

    -- Per-node detail is the part most likely to move under us, and the tree
    -- totals above are useful without it. Same bargain the talent reader
    -- strikes: if the node API throws, keep what was already read and record
    -- no nodes at all rather than a partial list.
    local ok, nodes = pcall(legacyNodes, configID, entry.treeID)
    if ok then tree.nodes = nodes end
    return tree
end

function legacyNodes(configID, treeID)
    local nodes = {}
    for _, nodeID in ipairs(C_Traits.GetTreeNodes(treeID) or {}) do
        local info = C_Traits.GetNodeInfo(configID, nodeID)
        if info then
            local name, spellID
            local entryID = (info.activeEntry and info.activeEntry.entryID)
                or (info.entryIDs and info.entryIDs[1])
            if entryID and has("C_Traits.GetEntryInfo") and has("C_Traits.GetDefinitionInfo") then
                local traitEntry = C_Traits.GetEntryInfo(configID, entryID)
                local definition = traitEntry and traitEntry.definitionID
                    and C_Traits.GetDefinitionInfo(traitEntry.definitionID)
                if definition then
                    spellID = definition.spellID
                    name = definition.overrideName
                    if name == "" then name = nil end
                end
            end
            nodes[#nodes + 1] = {
                nodeID = nodeID,
                name = name or spellName(spellID),
                spellID = spellID,
                rank = info.ranksPurchased or 0,
                maxRank = info.maxRanks,
            }
        end
    end
    return nodes
end

local function captureLegacy()
    if not has("C_Traits.GetConfigIDByTreeID") then return nil end
    if not has("C_Traits.GetTreeNodes") or not has("C_Traits.GetNodeInfo") then return nil end
    local trees = {}
    for _, entry in ipairs(LEGACY_TREES) do
        trees[#trees + 1] = legacyTree(entry)
    end
    return trees
end

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

    -- Per-talent detail is the newest and least proven part of this path, so
    -- a failure there costs the detail and never the totals.
    local ok, detail = pcall(traitNodeTalents, configID, treeID, groupIDs)
    if not ok then detail = nil end

    local trees = {}
    for index, group in ipairs(currencies) do
        local first = group.currencyInfos and group.currencyInfos[1]
        if names[index] then
            local tree = { name = names[index], pointsSpent = (first and first.spent) or 0 }
            local talents = detail and detail[groupIDs[index]]
            if talents and #talents > 0 then tree.talents = talents end
            trees[#trees + 1] = tree
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
        if name then
            local tree = { name = name, pointsSpent = pointsSpent or 0 }
            local ok, talents = pcall(tabTalents, index)
            if ok and talents then tree.talents = talents end
            trees[#trees + 1] = tree
        end
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

-- The same fact, reported two ways by two client families.
--
-- The Classic path gets "optimal"/"medium"/"easy"/"trivial" as strings from
-- `GetTradeSkillInfo`. The modern path gets `Enum.TradeskillRelativeDifficulty`,
-- a plain integer, and storing it raw put a number where every reader expects a
-- word — which the Rust side rejected outright, taking the whole file with it.
--
-- Normalising here is the same job `captureSkills` does for isCollapsed versus
-- isExpanded: one schema out of two client shapes. It is not a judgement. The
-- four names are the client's own, and this only translates between the two
-- ways it spells them.
--
-- An unrecognised value becomes nil rather than a guess. Absent already means
-- "not captured" everywhere in this schema, and the reader treats an unrated
-- recipe as one that might still raise the skill — the safe direction. A
-- client that adds a fifth state therefore degrades instead of lying.
local RELATIVE_DIFFICULTY = { [0] = "optimal", [1] = "medium", [2] = "easy", [3] = "trivial" }

local function relativeDifficultyName(value)
    if type(value) ~= "number" then return nil end
    return RELATIVE_DIFFICULTY[value]
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
                difficulty = relativeDifficultyName(recipe.relativeDifficulty),
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

    local professions, professionsReturned = captureProfessions()
    if professions then
        record.professions = professions
        -- How many values GetProfessions handed back, nils counted. The fact
        -- that tells an empty list apart from a broken one: a working client
        -- with no professions returns its full set of slots, all nil, where a
        -- stub returns nothing at all. Without this the two are identical in
        -- the file, which is how an empty list from the Anniversary client was
        -- once read as "this character has no professions" when it meant "this
        -- client's GetProfessions does nothing".
        record.professionsReturned = professionsReturned
    end

    local spells = captureSpells()
    if spells then record.spells = spells end

    local talents = captureTalents()
    if talents then record.talents = talents end

    local legacy = captureLegacy()
    if legacy then record.legacy = legacy end

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
