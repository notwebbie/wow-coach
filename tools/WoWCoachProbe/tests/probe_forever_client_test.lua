-- Smoke test 2: a populated Forever-like client, so the happy paths execute.
local messages = {}
DEFAULT_CHAT_FRAME = { AddMessage = function(_, m) messages[#messages+1] = m end }
function time() return 1700000000 end
function GetBuildInfo() return "1.60.1", "69913", "Sep 17 2026", 16001 end
WOW_PROJECT_ID = 1
function UnitName() return "Probehero" end
function UnitXP() return 100 end
function UnitXPMax() return 1000 end
function UnitLevel() return 12 end
function IsResting() return true end
function GetRestState() return 1, "Rested", 2 end
function GetXPExhaustion() return 500 end
function GetRealZoneText() return "Zephras Isle" end
function GetRealmName() return "Normal" end
function GetNormalizedRealmName() return "Normal" end
function GetProfessions() return 3, 5, nil, 8, 9 end
function GetProfessionInfo(i)
  return "Alchemy", 0, 210, 300, 0, 0, 171, 0, 0, 0, "Alchemy"
end

Enum = { GameRule = { HardcoreRuleset = 10, PvPRuleset = 213, RPRuleset = 214, SelfFoundAllowed = 22 } }
C_GameRules = {
  IsHardcoreActive = function() return false end,
  IsSelfFoundAllowed = function() return false end,
  IsGameRuleActive = function(rule) return rule == 213 end,
  GetActiveGameMode = function() return 1 end,
}
C_SkillInfo = {
  GetNumSkillLines = function() return 12 end,
  GetSkillLineInfo = function(i)
    return { skillID = 171, name = "Alchemy", isHeader = false, isCollapsed = false,
             rank = 210, tempPoints = 0, modifier = 0, maxRank = 300, parentSkillLineID = 0 }
  end,
  GetSkillLineInfoByID = function(id)
    return { skillID = id, name = "Defense", rank = 60, maxRank = 60, isCollapsed = false }
  end,
}
C_QuestLog = {
  GetNumQuestLogEntries = function() return 3, 2 end,
  GetInfo = function(i)
    if i == 1 then return { title = "Elwynn Forest", isHeader = true } end
    return { title = "Kobold Camp Cleanup", isHeader = false, questID = 62, level = 8 }
  end,
  IsComplete = function() return true end,
  IsFailed = function() return false end,
}
C_SpecializationInfo = {
  GetCombatConfigIDForSpecGroup = function() return 777 end,
  GetSpecialization = function() return 62 end,
}
C_ClassTalents = {
  GetActiveConfigID = function() return 777 end,
  HasUnspentTalentPoints = function() return true end,
  GetTraitTreeForSpec = function() return 900 end,
}
C_Traits = {
  GetConfigInfo = function(id) return { type = 1, name = "Talents", treeIDs = { 900 } } end,
  GetGroupDisplayInfoByTreeID = function() return {
    { groupID = 1, displayName = "Arcane" }, { groupID = 2, displayName = "Fire" },
    { groupID = 3, displayName = "Frost" } } end,
  GetGroupCurrencyInfo = function() return {
    { currencyInfos = { { spent = 0 } } }, { currencyInfos = { { spent = 21 } } },
    { currencyInfos = { { spent = 5 } } } } end,
  GetConfigIDByTreeID = function() return 555 end,
  GetTreeCurrencyInfo = function() return { { quantity = 4, maxQuantity = 65, spentInTree = 12 } } end,
  GetTreeNodes = function() return { 110298, 110299 } end,
  GetNodeInfo = function(_, n) return { entryIDs = { n * 10 }, ranksPurchased = 1, maxRanks = 3 } end,
  GetEntryInfo = function(_, e) return { definitionID = e } end,
  GetDefinitionInfo = function(d) return { overrideName = (d == 1102980) and "Well Rested" or "Other" } end,
}
C_MajorFactions = { GetCurrentRenownLevel = function() return 7 end }
C_TradeSkillUI = {
  IsTradeSkillReady = function() return true end,
  GetBaseProfessionInfo = function() return { professionName = "Alchemy", skillLevel = 210,
                                              maxSkillLevel = 300, professionID = 171 } end,
  GetAllRecipeIDs = function() return { 2330, 2337, 3447 } end,
  GetRecipeInfo = function() return { categoryID = 1, name = "Minor Healing Potion",
                                      relativeDifficulty = 2, numSkillUps = 1, canSkillUp = true } end,
}
C_UnitAuras = { GetAuraDataByIndex = function(_, i)
  if i > 2 then return nil end
  return { spellId = 7000 + i, name = "Campfire Warmth", sourceUnit = "player" }
end }
SlashCmdList = {}
local frame = { events = {}, scripts = {} }
function frame:RegisterEvent(e) self.events[e] = true end
function frame:SetScript(s, h) self.scripts[s] = h end
function CreateFrame() return frame end

assert(loadfile("WoWCoachProbe.lua"))("WoWCoachProbe")
frame.scripts.OnEvent(frame, "PLAYER_LOGIN")
local ok, err = pcall(SlashCmdList["WCPROBE"], "")
assert(ok, "runAll errored on a populated client: " .. tostring(err))
for _, sub in ipairs({"legacy", "auras", "recipes", "secret", "report"}) do
  local ok2, err2 = pcall(SlashCmdList["WCPROBE"], sub)
  assert(ok2, "/wcprobe " .. sub .. " errored: " .. tostring(err2))
end

local r = WoWCoachProbeDB.runs[1].results
local function val(k) return r[k] and tostring(r[k].value or r[k].detail or r[k].status) or "MISSING" end
assert(r["client.hasSkillInfo"].value == "true", "sentinel should detect C_SkillInfo")
assert(r["skills.count"].value == 12, "skill count should be read")
assert(val("ruleset.isActive.PvPRuleset"):find("true"), "PvP ruleset should be detected")
assert(val("ruleset.isActive.RPRuleset"):find("false"), "RP ruleset should be false")
assert(val("talents.groups"):find("Fire spent=21"), "talent group spend should be read: " .. val("talents.groups"))
assert(val("legacy.adventure"):find("available=4"), "legacy currency should be read")
assert(val("recipes.allRecipeIDs"):find("3 recipe"), "recipe ids should be counted")
assert(r["secret.concat"].status == "ok", "concat probe should run")
-- Well Rested must be findable by name in the node dump.
local found
for _, node in ipairs(WoWCoachProbeDB.legacyAdventureNodes or {}) do
  if node.name == "Well Rested" then found = node end
end
assert(found, "Well Rested node should be identified in the dump")
assert(#WoWCoachProbeDB.auraSnapshots >= 1 and #WoWCoachProbeDB.auraSnapshots[1].auras == 2,
       "aura snapshot should capture buffs")
print("smoke2: populated client OK — sentinel, skills, ruleset, talents, legacy, recipes, auras all read")
print("        Well Rested found at nodeID " .. found.nodeID)
