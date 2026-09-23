local messages = {}
DEFAULT_CHAT_FRAME = { AddMessage = function(_, m) messages[#messages+1] = m end }
function time() return 1700000000 end
function GetBuildInfo() return "1.60.1","69913","Sep 17 2026",16001 end
WOW_PROJECT_ID = 1
function UnitName() return "Trollmage" end
function UnitExists() return false end
function UnitXP() return 40 end function UnitXPMax() return 400 end
function UnitLevel() return 1 end function IsResting() return false end
function GetRestState() return 2 end function GetXPExhaustion() return nil end
function GetRealZoneText() return "Durotar" end
function GetRealmName() return "Classic Beta PvE" end
-- Mirrors the live beta exactly: these return ZERO values with no pet out.
C_PetInfo = {
  GetPetHappiness = function() return end,
  GetPetLoyalty = function() return end,
  GetPetTrainingPoints = function() return 0, 0 end,
  GetPetFoodTypes = function() return {} end,
}
C_StableInfo = { GetNumStableSlots = function() return 0 end }
-- Talent config carrying its own treeIDs, the route the old code missed.
C_SpecializationInfo = { GetCombatConfigIDForSpecGroup = function() return 6788875 end,
                         GetSpecialization = function() return 1 end }
C_Traits = {
  GetConfigInfo = function(id) return { type = 1, name = "Talents", treeIDs = { 900 } } end,
  GetGroupDisplayInfoByTreeID = function() return { { groupID = 1, displayName = "Frost" } } end,
  GetGroupCurrencyInfo = function() return { { currencyInfos = { { spent = 7 } } } } end,
  GetConfigIDByTreeID = function() return 6788879 end,
  GetTreeCurrencyInfo = function() return { { quantity = 0, maxQuantity = 0, spentInTree = 0 } } end,
}
C_ClassTalents = { GetActiveConfigID = function() return 6788875 end,
                   HasUnspentTalentPoints = function() return false end }
SlashCmdList = {}
local frame = { events = {}, scripts = {} }
function frame:RegisterEvent(e) self.events[e] = true end
function frame:SetScript(s,h) self.scripts[s] = h end
function CreateFrame() return frame end
assert(loadfile("WoWCoachProbe.lua"))("WoWCoachProbe")
frame.scripts.OnEvent(frame, "PLAYER_LOGIN")
assert(pcall(SlashCmdList["WCPROBE"], ""), "runAll errored")
local r = WoWCoachProbeDB.runs[1].results
assert(r["pet.loyalty"].status ~= "error", "pet.loyalty must not error with no pet: " .. tostring(r["pet.loyalty"].detail))
assert(tostring(r["pet.loyalty"].value):find("no value"), "should report the empty return")
assert(tostring(r["talents.groups"].value):find("Frost spent=7"),
       "talent groups should resolve via config treeIDs: " .. tostring(r["talents.groups"].value))
assert(tostring(r["talents.configInfo"].value):find("treeIDs=900"), "config info should list tree ids")
print("nopet_test: loyalty handled, talent tree resolved via GetConfigInfo")
