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
    -- isComplete for quest 9440, a number on this client: 1 complete, nil active.
    quest_9440_complete = 1,
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
function GetNumSkillLines() return 5 end
function GetSkillLineInfo(index)
    if index == 1 then
        return "Professions", true, world.skill_header_expanded, 0, 0, 0, 0
    elseif index == 2 then
        return "Alchemy", false, true, 225, 10, 0, 300
    elseif index == 3 then
        return "Herbalism", false, true, 150, 0, 0, 300
    elseif index == 4 then
        -- A second header, so the grouping has to actually track which one a
        -- skill followed rather than tagging everything with the first.
        return "Weapon Skills", true, true, 0, 0, 0, 0
    end
    return "Daggers", false, true, 100, 0, 0, 100
end

-- Spellbook: two tabs, and one FUTURESPELL among them.
--
-- The greyed-out entry is the point of this stub. The spellbook lists spells
-- the character is high enough for but has not trained, and recording one
-- turns "could learn this" into "knows this" — a claim a player would act on
-- by walking to a city they cannot actually reach.
local spellbook = {
    -- index = { skillType, spellID, name }
    [1] = { "SPELL", 133, "Fireball" },
    [2] = { "SPELL", 3567, "Teleport: Orgrimmar" },
    [3] = { "FUTURESPELL", 3566, "Teleport: Thunder Bluff" },
    [4] = { "SPELL", 1449, "Arcane Explosion" },
    -- A flyout container rather than a castable spell.
    [5] = { "FLYOUT", 9999, "Teleports" },
    -- The same spell again, as a second tab can list it.
    [6] = { "SPELL", 133, "Fireball" },
}
function GetNumSpellTabs() return 2 end
function GetSpellTabInfo(tab)
    if tab == 1 then return "General", nil, 0, 4 end
    return "Arcane", nil, 4, 2
end
function GetSpellBookItemInfo(index, bookType)
    assert(bookType == "spell")
    local entry = spellbook[index]
    if not entry then return nil end
    return entry[1], entry[2]
end
function GetSpellBookItemName(index, bookType)
    assert(bookType == "spell")
    local entry = spellbook[index]
    if not entry then return nil end
    return entry[3]
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

-- Per-talent detail: only the Frost tab is populated, so the other two show
-- that an empty tab stores no list rather than an empty one. Returns are
-- name, icon, tier, column, rank, maxRank — the Classic order.
local frost_talents = {
    { "Frost Warding", "icon", 1, 1, 0, 2 },
    { "Improved Frostbolt", "icon", 1, 2, 5, 5 },
    { "Elemental Precision", "icon", 1, 3, 0, 3 },
    { "Ice Shards", "icon", 2, 1, 0, 5 },
    { "Permafrost", "icon", 2, 4, 0, 3, requires = { 1, 2 } },
}
function GetNumTalents(tab) return tab == 3 and #frost_talents or 0 end
function GetTalentInfo(tab, index)
    if tab ~= 3 then return nil end
    local t = frost_talents[index]
    if not t then return nil end
    return t[1], t[2], t[3], t[4], t[5], t[6], false, true
end
function GetTalentPrereqs(tab, index)
    local t = tab == 3 and frost_talents[index]
    if t and t.requires then return t.requires[1], t.requires[2], true end
    return nil
end

-- Quest log: header, two quests. questID is return 8, isComplete is a NUMBER.
function GetNumQuestLogEntries() return 3, 2 end
function GetQuestLogTitle(index)
    if index == 1 then
        return "Hellfire Peninsula", 0, nil, true, world.quest_header_collapsed, nil, 0, 0
    elseif index == 2 then
        return "Cleansing the Waters", 62, nil, false, false, world.quest_9440_complete, 0, 9440
    end
    return "Failed Errand", 60, nil, false, false, -1, 0, 9441
end

-- Containers: backpack plus one bag, and a bank behind them. Modern
-- C_Container only; the globals are absent, matching the 2.5.x client.
--
-- `bank_open` models the one property that makes the bank unlike the bags, and
-- it models the nastier half of it deliberately. Away from a banker the client
-- still knows how big the bank is, so the slot count comes back correct and
-- only the items come back nil — the bank looks present and empty rather than
-- absent. A stub that returned "no slots" when shut would be the easy case and
-- would let a broken guard pass, so this one reproduces the trap: slots yes,
-- contents no.
local bank_open = false
local bag_slots = { [0] = 16, [1] = 6, [-1] = 24, [5] = 16 }
local bag_items = {
    [0] = { [1] = { 2589, 20 }, [2] = { 2592, 5 } },
    [1] = { [1] = { 2589, 12 } },
    [-1] = { [1] = { 2589, 100 }, [2] = { 2772, 40 } },
    [5] = { [1] = { 2772, 20 } },
}
local function is_bank(bag) return bag == -1 or bag == -3 or (bag >= 5 and bag <= 11) end
-- `torn_down` models the client around a loading screen or logout, the trap a
-- live capture fell into on 22 October: sizes and free counts still answer,
-- and every item lookup comes back nil. `vanished` lists bags whose slot count
-- has gone to zero while the bag itself is still reported as equipped.
local torn_down = false
local vanished = {}
local function readable(bag)
    if torn_down then return false end
    return bank_open or not is_bank(bag)
end
C_Container = {}
-- The size is known whether or not the window is open.
function C_Container.GetContainerNumSlots(bag)
    if vanished[bag] then return 0 end
    return bag_slots[bag] or 0
end
function C_Container.GetContainerNumFreeSlots(bag)
    local slots = bag_slots[bag] or 0
    if vanished[bag] then return 0, 0 end
    -- A shut bank reports everything free; a torn-down bag still reports the
    -- true free count, which is exactly what makes the two distinguishable.
    if not torn_down and not readable(bag) then return slots, 0 end
    local used = 0
    for _ in pairs(bag_items[bag] or {}) do used = used + 1 end
    return slots - used, 0
end
-- The contents are not.
function C_Container.GetContainerItemInfo(bag, slot)
    if not readable(bag) then return nil end
    local entry = (bag_items[bag] or {})[slot]
    if not entry then return nil end
    return { itemID = entry[1], stackCount = entry[2] }
end
function C_Container.ContainerIDToInventoryID(bag) return 19 + bag end
-- Item names come from the client's cache and are absent for anything it has
-- not loaded. Only one of the two stacks is known here, which is the normal
-- state rather than an edge case.
C_Item = C_Item or {}

-- A believable wardrobe for a level 34 mage, with two deliberate holes.
--
-- The RANGED slot is empty: a mage at 34 with no wand is the case the gear
-- rules exist to catch, and the fixture should carry it rather than make the
-- test invent one. Slots 13 and 14 hold items the client has NOT cached, so
-- they arrive as an id and nothing else — which is a third state the rules
-- have to handle, between "a wand is equipped" and "the slot is empty".
local WARDROBE = {
    [1]  = { id = 7372,  "Magician's Hat",       2, 36, 31, "Armor",  "Cloth", "INVTYPE_HEAD" },
    [5]  = { id = 6479,  "Robes of Arcana",      2, 35, 30, "Armor",  "Cloth", "INVTYPE_ROBE" },
    [8]  = { id = 4119,  "Sorcerer Slippers",    1, 32, 27, "Armor",  "Cloth", "INVTYPE_FEET" },
    [10] = { id = 2583,  "Gloves of Meditation", 1, 30, 25, "Armor",  "Cloth", "INVTYPE_HAND" },
    [13] = { id = 9999,  nil },
    [14] = { id = 9998,  nil },
    [16] = { id = 1482,  "Gnarled Ash Staff",    2, 34, 29, "Weapon", "Staves", "INVTYPE_2HWEAPON" },
}

function C_Item.GetItemInfo(itemID)
    if itemID == 2589 then return "Linen Cloth" end
    for _, entry in pairs(WARDROBE) do
        if entry.id == itemID and entry[1] then
            return entry[1], nil, entry[2], entry[3], entry[4], entry[5], entry[6], nil, entry[7]
        end
    end
    return nil
end
-- `stripped` models the logout case for equipment: the call keeps answering
-- and answers nothing, which read as a fact says the character is naked.
local stripped = false
function GetInventoryItemID(unit, slot)
    assert(unit == "player")
    if stripped then return nil end
    -- Above 19 are the bag slots, which the container code reads through this
    -- same call and which must keep answering.
    if slot > 19 then return 4000 + slot end
    local entry = WARDROBE[slot]
    return entry and entry.id or nil
end

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
-- Reagents: Elixir of Fortitude (index 2) takes 1 Goldthorn and 2 Stranglekelp.
function GetTradeSkillNumReagents(index)
    if index == 2 then return 2 end
    return 1
end
function GetTradeSkillReagentInfo(index, slot)
    if index == 2 and slot == 1 then return "Goldthorn", "tex", 1, 5 end
    if index == 2 and slot == 2 then return "Stranglekelp", "tex", 2, 0 end
    return "Peacebloom", "tex", 1, 3
end
function GetTradeSkillReagentItemLink(index, slot)
    if index == 2 and slot == 1 then return "|cff00ff00|Hitem:3356:0:0:0|h[Goldthorn]|h|r" end
    if index == 2 and slot == 2 then return "|cff00ff00|Hitem:3820:0:0:0|h[Stranglekelp]|h|r" end
    return "|cff00ff00|Hitem:2447:0:0:0|h[Peacebloom]|h|r"
end
function GetTradeSkillItemLink(index)
    if index == 2 then return "|cff00ff00|Hitem:3825:0:0:0|h[Elixir of Fortitude]|h|r" end
    return "|cff00ff00|Hitem:929:0:0:0|h[Healing Potion]|h|r"
end
function GetTradeSkillNumMade(index)
    if index == 2 then return 1, 1 end
    return 1, 2
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
-- An enchant consumes reagents but produces no item.
function GetCraftNumReagents(index) return 1 end
function GetCraftReagentInfo(index, slot) return "Strange Dust", "tex", 3, 12 end
function GetCraftReagentItemLink(index, slot)
    return "|cffffffff|Hitem:10940:0:0:0|h[Strange Dust]|h|r"
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

-- Arriving in the world queues a capture rather than taking one: the loading
-- screen lifts before bag and item data have finished arriving.
frame:Fire("PLAYER_LOGIN")
check(WoWCoachCollectorDB == nil or WoWCoachCollectorDB.characters == nil
        or WoWCoachCollectorDB.characters[key] == nil,
    "login must not capture on the spot")
frame:Tick(5)
local snap = record()

check(snap.schemaVersion == 3, "schema version should be 3")
check(snap.capturedAt == world.now, "capturedAt should be the epoch time")

-- Equipment. Every slot the stub answers for is captured, and the slot number
-- is stored as a number rather than interpreted: which slot holds a wand is
-- the core's problem, not the collector's.
check(snap.equipment ~= nil, "equipment should be captured")
check(#snap.equipment.slots == 7, "only the slots holding something are listed")
check(snap.equipment.slotsRead == 19,
    "and how many were asked for is recorded, so an empty list can be read as naked")

local function worn(slot)
    for _, entry in ipairs(snap.equipment.slots) do
        if entry.slot == slot then return entry end
    end
    return nil
end

check(worn(1) and worn(1).itemID == 7372 and worn(1).name == "Magician's Hat",
    "a cached item carries its name beside the id")
check(worn(1).itemSubType == "Cloth" and worn(1).equipLoc == "INVTYPE_HEAD",
    "and the client's own type and slot-kind, so a reader need not infer either")
check(worn(13) and worn(13).itemID == 9999 and worn(13).name == nil,
    "an uncached item stores its id alone rather than being dropped")
check(worn(18) == nil, "an empty slot is absent from the list, not present and null")
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
-- A name is a convenience; the id is the fact. An item the client has not
-- loaded is written without one rather than delaying or failing the capture,
-- and that is the common case at logout, not a rare one.
local stacks = snap.inventory and snap.inventory.contents
local named, nameless
for _, stack in ipairs(stacks or {}) do
    if stack.itemID == 2589 then named = stack end
    if stack.itemID == 2592 then nameless = stack end
end
check(named and named.name == "Linen Cloth", "a cached item should carry its name")
check(nameless and nameless.name == nil,
    "an item the client has not loaded must be written without a name, not with a placeholder")
check(nameless and nameless.itemID == 2592, "and its id is still the fact")

check(snap.skills and #snap.skills == 3, "headers should be excluded from skills")
-- The header is the game's own answer to what kind of skill each one is.
-- Throwing it away and guessing the answer back from the name works in
-- English and nowhere else.
check(snap.skills and snap.skills[1].header == "Professions",
    "a skill should carry the header it sat under")
check(snap.skills and snap.skills[3].header == "Weapon Skills",
    "and the header must track, not stick at the first one seen")
check(snap.skills and snap.skills[3].name == "Daggers",
    "the second group's skills should still be captured")
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
local frost_tree
for _, tree in ipairs(snap.talents and snap.talents.trees or {}) do
    if tree.name == "Frost" then frost_tree = tree end
    if tree.name == "Arcane" then
        check(tree.talents == nil, "a tab with no talents stores no list")
    end
end
check(frost_tree and frost_tree.talents and #frost_tree.talents == 5, "every talent in the tab captured")
local frostbolt = frost_tree and frost_tree.talents and frost_tree.talents[2]
check(frostbolt and frostbolt.name == "Improved Frostbolt", "talent name captured")
check(frostbolt and frostbolt.tier == 1 and frostbolt.column == 2, "tier and column captured")
check(frostbolt and frostbolt.rank == 5 and frostbolt.maxRank == 5, "rank and maxRank captured")
local warding = frost_tree and frost_tree.talents and frost_tree.talents[1]
check(warding and warding.rank == 0, "an unlearned talent is stored with rank 0, not dropped")
local permafrost = frost_tree and frost_tree.talents and frost_tree.talents[5]
check(permafrost and permafrost.requires and permafrost.requires.tier == 1
    and permafrost.requires.column == 2, "prerequisite arrow captured as tier and column")
check(warding and warding.requires == nil, "no prerequisite is stored as absent")

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
check(snap.inventory and snap.inventory.contents and #snap.inventory.contents == 2,
    "the bag capture must not reach into the bank")

-- Spells
local spells = snap.spells
check(spells ~= nil, "the spellbook should be captured")
local byID = {}
for _, entry in ipairs(spells or {}) do byID[entry.spellID] = entry end
check(byID[133] ~= nil, "a known spell should be recorded")
check(byID[133] and byID[133].name == "Fireball", "the cached name should come along")
check(byID[3567] ~= nil, "a known teleport should be recorded")
check(byID[3566] == nil, "a FUTURESPELL is eligible, not known, and must not be recorded")
check(byID[9999] == nil, "a flyout container is not a spell the character can cast")
check(#(spells or {}) == 3, "every tab is walked, and a spell listed twice is recorded once")
-- The collector must not decide what a teleport is; that rule lives in the core.
check(snap.teleports == nil and snap.travel == nil,
    "the collector must not classify spells")

--------------------------------------------------------------------------------
-- Bank
--
-- The bank is only readable at a banker, so every assertion here is really
-- about when the addon looks, not about what it finds.
--------------------------------------------------------------------------------

check(record().bank == nil, "the bank must not be captured by an ordinary snapshot")

-- A slot-changed event away from the bank must not be taken as permission to
-- read. This is the case that would silently erase a mule's contents.
frame:Fire("PLAYERBANKSLOTS_CHANGED")
check(record().bank == nil, "a slot event alone must not be read as the bank being open")

bank_open = true
frame:Fire("BANKFRAME_OPENED")
local bank = record().bank
check(bank ~= nil, "opening the bank must capture it")
check(bank and bank.totalSlots == 40, "bank slots should sum the main window and its bags")
check(bank and bank.capturedAt ~= nil, "a bank snapshot must record when it was taken")
local banked_linen, banked_wool
for _, item in ipairs(bank and bank.contents or {}) do
    if item.itemID == 2589 then banked_linen = item end
    if item.itemID == 2772 then banked_wool = item end
end
check(banked_linen and banked_linen.count == 100, "bank contents should be captured")
check(banked_wool and banked_wool.count == 60, "stacks must be summed across bank bags")

-- Closing the bank must not discard what was seen. Forgetting a mule's
-- contents the moment the player walks away would make the capture useless for
-- the one character it exists to serve.
frame:Fire("BANKFRAME_CLOSED")
bank_open = false
frame:Fire("PLAYERBANKSLOTS_CHANGED")
check(record().bank ~= nil, "the last good bank look must survive the window closing")
-- Checking the contents, not the slot count: a reading taken with the bank
-- shut reports the right size and nothing in it, so a size assertion would
-- pass while the holding was erased. That is the failure this guards.
check(record().bank and record().bank.contents and #record().bank.contents == 2,
    "and must not be overwritten by a reading taken with the bank shut")

-- A client with no bank at all must leave the last good look alone rather than
-- replace it with nothing.
local saved_slots = bag_slots
bag_slots = { [0] = 16, [1] = 6 }
bank_open = true
frame:Fire("BANKFRAME_OPENED")
check(record().bank and record().bank.contents and #record().bank.contents == 2,
    "a bank that reports no containers must not erase what was already seen")
bag_slots = saved_slots
bank_open = false

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
-- Loading screens and logout
--
-- Replays the 22 October capture: Dardring in Tanaris, backpack 20 slots with
-- 2 free and no items readable, bags 1-4 gone, and two collection quests read
-- back as active. Every assertion here is about refusing to believe that.
--------------------------------------------------------------------------------

local function linen_count()
    for _, item in ipairs(record().inventory and record().inventory.contents or {}) do
        if item.itemID == 2589 then return item.count end
    end
    return nil
end
local function quest_state(questID)
    for _, quest in ipairs(record().quests or {}) do
        if quest.questID == questID then return quest.state end
    end
    return nil
end

check(linen_count() == 32 and quest_state(9440) == "complete", "baseline before the teardown cases")

-- 1. Occupied slots that return no items: refuse the whole reading.
torn_down = true
world.quest_9440_complete = nil   -- the client stops counting the quest's items too
world.money = 333333
frame:Fire("BAG_UPDATE_DELAYED")
frame:Tick(10)
check(record().moneyCopper == 333333, "a refused bag reading must not stop the rest of the capture")
check(linen_count() == 32, "occupied slots with no items are a torn-down read, not an empty bag")
check(record().inventory and record().inventory.totalSlots == 22,
    "the last good inventory must stay whole, bags and all")
check(quest_state(9440) == "complete",
    "a collection quest must not drop back to active on a read whose bags were refused")
torn_down = false

-- 1b. The same moment, read through the equipment slots: every call answers
-- nothing. Stored as a fact that says the character is wearing nothing, which
-- would have the coach telling a geared warlock to go and find a wand.
local worn_before = #record().equipment.slots
stripped = true
torn_down = true
frame:Fire("BAG_UPDATE_DELAYED")
frame:Tick(10)
check(record().equipment and #record().equipment.slots == worn_before,
    "an equipment read that answers for nothing while the bags are torn down is refused")
torn_down = false

-- And on its own: bags fine, equipment silent, but a previous reading saw
-- items. A character does not strip between two captures.
frame:Fire("BAG_UPDATE_DELAYED")
frame:Tick(10)
check(record().equipment and #record().equipment.slots == worn_before,
    "nothing worn where something was worn before is refused even with good bags")
-- 1c. The case the bag cross-check exists for, and the only one the
-- previous-reading check cannot see: a character captured for the FIRST time
-- during a teardown. There is no earlier reading to disagree with, so without
-- the bags' verdict an empty answer becomes that character's opening record —
-- a brand new warlock filed as owning nothing, which is how the Dardring bug
-- started.
WoWCoachCollectorDB.characters[key] = nil
torn_down = true
frame:Fire("BAG_UPDATE_DELAYED")
frame:Tick(10)
local first = WoWCoachCollectorDB.characters[key]
check(first == nil or first.equipment == nil,
    "a first capture during teardown must not record an empty wardrobe as fact")
torn_down = false
stripped = false

frame:Fire("BAG_UPDATE_DELAYED")
frame:Tick(10)
check(#record().equipment.slots == worn_before, "and the reading returns once the client answers again")

-- 2. A bag still equipped as the same item cannot shrink to no slots.
vanished[1] = true
frame:Fire("BAG_UPDATE_DELAYED")
frame:Tick(10)
check(record().inventory and #record().inventory.bags == 2,
    "an equipped bag reporting no slots must not be dropped from the inventory")
check(linen_count() == 32, "and its contents must not be lost with it")
vanished[1] = nil

-- 3. With readable bags, complete -> active is real and must be recorded:
-- the items were sold or destroyed, and the coach has to know.
frame:Fire("QUEST_LOG_UPDATE")
frame:Tick(10)
check(quest_state(9440) == "active",
    "with readable bags a quest that is no longer complete must be recorded as active")
world.quest_9440_complete = 1
frame:Fire("QUEST_LOG_UPDATE")
frame:Tick(10)
check(quest_state(9440) == "complete", "and completing it again is picked up")

-- 4. Leaving the world drops a queued capture and ignores everything after.
world.money = 444444
frame:Fire("BAG_UPDATE_DELAYED")          -- queued, due in five seconds
frame:Fire("PLAYER_LEAVING_WORLD")
torn_down = true
frame:Fire("BAG_UPDATE_DELAYED")          -- the burst a logout produces
frame:Fire("QUEST_LOG_UPDATE")
frame:Tick(10)
check(record().moneyCopper == 333333, "nothing may be captured between leaving and entering the world")
frame:Fire("PLAYER_LOGOUT")
frame:Tick(10)
check(record().moneyCopper == 333333, "and nothing after logout")

-- 5. Arriving back defers past the loading screen, then captures normally.
torn_down = false
frame:Fire("PLAYER_ENTERING_WORLD")
check(record().moneyCopper == 333333, "entering the world must not capture on the spot")
frame:Tick(2)
check(record().moneyCopper == 333333, "nor before the throttle has run")
frame:Tick(5)
check(record().moneyCopper == 444444, "a capture must follow once the loading screen is behind it")
check(linen_count() == 32, "with the bags read properly again")
-- Put the money back where the fixture expects it.
world.money = 222222
frame:Fire("PLAYER_MONEY")
frame:Tick(10)

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

-- Reagents are the difference between "knows the recipe" and "can make it".
local elixir = recipes and recipes.Alchemy and recipes.Alchemy.recipes[1]
check(elixir and elixir.reagents and #elixir.reagents == 2,
    "both reagent slots should be captured")
check(elixir and elixir.reagents and elixir.reagents[1].itemID == 3356,
    "reagent item id should be parsed out of the reagent link")
check(elixir and elixir.reagents and elixir.reagents[2].count == 2,
    "the required count, not the carried count, is what belongs in a recipe")
check(elixir and elixir.makesItemID == 3825,
    "the produced item is what lets a craft be valued")
check(elixir and elixir.makesMin == 1 and elixir.makesMax == 1,
    "how many it makes bounds the value")
local potion = recipes and recipes.Alchemy and recipes.Alchemy.recipes[2]
check(potion and potion.makesMax == 2, "a variable yield keeps both bounds")
-- playerReagentCount is a fact about one bag at one moment, and the bag
-- capture already answers that for every character. Storing it here would put
-- a second, staler answer to the same question in the file.
check(elixir and elixir.reagents and elixir.reagents[1].playerCount == nil,
    "the crafter's carried count must not be persisted with the recipe")

-- TBC keeps Enchanting on the separate Craft API.
trade_open = false
craft_open = true
frame:Fire("CRAFT_SHOW")
recipes = record().recipes
check(recipes and recipes.Enchanting, "craft capture should store Enchanting")
check(recipes and recipes.Enchanting and #recipes.Enchanting.recipes == 1, "craft headers should be excluded")
local enchant = recipes and recipes.Enchanting and recipes.Enchanting.recipes[1]
check(enchant and enchant.reagents and enchant.reagents[1].itemID == 10940,
    "craft reagents should be captured on the Craft API too")
check(enchant and enchant.reagents and enchant.reagents[1].count == 3,
    "craft reagent counts should be captured")
check(enchant and enchant.makesItemID == nil,
    "an enchant produces no item and none should be invented")
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

--------------------------------------------------------------------------------
-- A GetProfessions that exists and answers nothing
--
-- The Anniversary client (interface 20506) has this function and it returns no
-- values at all. Two characters carrying Tailoring, Enchanting, Mining and
-- Skinning between them both captured as an empty list, and the schema at the
-- time said an empty list meant "this character has no professions". It does
-- not. The arity is what tells them apart, so it is what gets asserted.
--------------------------------------------------------------------------------

function GetProfessions() return end
function GetProfessionInfo(index) return nil end

frame:Fire("PLAYER_ENTERING_WORLD")
frame:Tick(10)
local stub = record()
check(stub.professions ~= nil and #stub.professions == 0,
    "a stub returns an empty list, not nil -- the capability check passed")
check(stub.professionsReturned == 0,
    "and the arity says it answered with nothing, which is how a reader knows")

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
