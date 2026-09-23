local messages = {}
DEFAULT_CHAT_FRAME = { AddMessage = function(_, m) messages[#messages+1] = m end }
function time() return 1700000000 end
function GetBuildInfo() return "1.60.1", "69913", "Sep 17 2026", 16001 end
WOW_PROJECT_ID = 1
function UnitName() return "Trollmage" end
function UnitXP() return 40 end
function UnitXPMax() return 400 end
function UnitLevel() return 1 end
function IsResting() return false end        -- not in an inn
function GetRestState() return 2, "Normal", 1 end
function GetXPExhaustion() return nil end    -- no rested bonus
function GetRealZoneText() return "Durotar" end
function GetRealmName() return "Classic Beta PvE" end
SlashCmdList = {}
local frame = { events = {}, scripts = {} }
function frame:RegisterEvent(e) self.events[e] = true end
function frame:SetScript(s, h) self.scripts[s] = h end
function CreateFrame() return frame end
assert(loadfile("WoWCoachProbe.lua"))("WoWCoachProbe")
frame.scripts.OnEvent(frame, "PLAYER_LOGIN")
local s = WoWCoachProbeDB.restedSamples[1]
assert(s.isResting == false, "isResting must be recorded as false, not dropped (got " .. tostring(s.isResting) .. ")")
assert(s.exhaustion == 0, "exhaustion must be recorded as 0, not dropped (got " .. tostring(s.exhaustion) .. ")")
assert(s.restState == 2, "restState should be 2")
print("resting_test: not-resting sample keeps isResting=false and exhaustion=0")
