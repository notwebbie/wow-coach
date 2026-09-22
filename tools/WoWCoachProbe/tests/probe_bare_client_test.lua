-- Smoke test: run the probe on a client where almost nothing exists.
-- It must complete without erroring and record the absences as results.
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
SlashCmdList = {}
local frame = { events = {}, scripts = {} }
function frame:RegisterEvent(e) self.events[e] = true end
function frame:SetScript(s, h) self.scripts[s] = h end
function CreateFrame() return frame end

assert(loadfile("WoWCoachProbe.lua"))("WoWCoachProbe")
frame.scripts.OnEvent(frame, "PLAYER_LOGIN")
local ok, err = pcall(SlashCmdList["WCPROBE"], "")
assert(ok, "runAll errored on a bare client: " .. tostring(err))
for _, sub in ipairs({"secret", "auras", "recipes", "legacy", "report"}) do
  local ok2, err2 = pcall(SlashCmdList["WCPROBE"], sub)
  assert(ok2, "/wcprobe " .. sub .. " errored: " .. tostring(err2))
end
assert(#WoWCoachProbeDB.runs == 1, "expected one recorded run")
assert(#WoWCoachProbeDB.restedSamples >= 2, "rested sampling did not record")
local r = WoWCoachProbeDB.runs[1].results
assert(r["client.hasSkillInfo"].value == "false", "sentinel should report absent C_SkillInfo")
assert(r["skills.count"].status ~= "ok", "absent API should not report ok")
print("smoke: survived a bare client; " .. #messages .. " chat lines, " ..
      #WoWCoachProbeDB.restedSamples .. " rested samples")
