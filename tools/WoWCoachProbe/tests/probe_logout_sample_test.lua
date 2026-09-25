-- The rested rate is fitted from the pair of samples either side of a logout,
-- because rest accrues while logged out. If nothing is recorded *at* logout,
-- the elapsed time is measured from whenever an event last happened to fire —
-- which can be hours early, and makes the fitted rate quietly wrong rather
-- than obviously missing.
--
-- This also checks the heartbeat, which is what proves the addon was still
-- running through a long quiet session rather than having stopped.

local messages = {}
DEFAULT_CHAT_FRAME = { AddMessage = function(_, m) messages[#messages+1] = m end }

local now = 1700000000
function time() return now end
function GetBuildInfo() return "1.60.1", "69913", "Sep 17 2026", 16001 end
WOW_PROJECT_ID = 1
function UnitName() return "Trollmage" end
function UnitXP() return 40 end
function UnitXPMax() return 400 end
function UnitLevel() return 1 end
function IsResting() return true end
function GetRestState() return 1, "Rested", 1 end
function GetXPExhaustion() return 120 end
function GetRealZoneText() return "Durotar" end
function GetRealmName() return "Classic Beta PvE" end
SlashCmdList = {}

-- A ticker that records its callback so the test can fire it by hand.
local tickers = {}
C_Timer = {
    NewTicker = function(seconds, callback)
        tickers[#tickers + 1] = { seconds = seconds, callback = callback }
        return { Cancel = function() end }
    end,
}

local frame = { events = {}, scripts = {} }
function frame:RegisterEvent(e) self.events[e] = true end
function frame:SetScript(s, h) self.scripts[s] = h end
function CreateFrame() return frame end

assert(loadfile("WoWCoachProbe.lua"))("WoWCoachProbe")

assert(frame.events.PLAYER_LOGOUT, "PLAYER_LOGOUT must be registered or the logout sample never happens")

frame.scripts.OnEvent(frame, "PLAYER_LOGIN")
assert(#tickers == 1, "login should start exactly one heartbeat ticker, got " .. #tickers)
assert(tickers[1].seconds >= 60, "a heartbeat faster than a minute is noise, not data")

-- A quiet session: nothing changes, so only the heartbeat fires.
now = now + tickers[1].seconds
tickers[1].callback()
now = now + tickers[1].seconds
tickers[1].callback()

-- Then the player logs out.
now = now + 30
frame.scripts.OnEvent(frame, "PLAYER_LOGOUT")

local samples = WoWCoachProbeDB.restedSamples
local last = samples[#samples]
assert(last.reason == "PLAYER_LOGOUT",
    "the final sample must be the logout one, got " .. tostring(last.reason))
assert(last.at == now, "the logout sample must carry the logout time")
assert(last.exhaustion == 120, "and the exhaustion held at that moment")

local heartbeats = 0
for _, sample in ipairs(samples) do
    if sample.reason == "heartbeat" then heartbeats = heartbeats + 1 end
end
assert(heartbeats == 2, "a quiet session should still leave a trail, got " .. heartbeats)

print("logout_test: logout sampled at logout time, heartbeat keeps a quiet session traceable")
