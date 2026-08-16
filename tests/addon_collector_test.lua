local addon_path = assert(arg[1], "usage: lua addon_collector_test.lua <addon.lua>")

local api = {
    xp = 3210,
    max_xp = 7800,
    money = 456789,
}

WOW_PROJECT_ID = 2
WOW_PROJECT_CLASSIC = 1
WOW_PROJECT_BURNING_CRUSADE_CLASSIC = 2
WOW_PROJECT_MAINLINE = 3

function UnitName(unit)
    assert(unit == "player")
    return "Testhero"
end

function GetRealmName()
    return "Test Realm"
end

function UnitClass(unit)
    assert(unit == "player")
    return "Mage", "MAGE", 8
end

function UnitRace(unit)
    assert(unit == "player")
    return "Human", "Human", 1
end

function UnitFactionGroup(unit)
    assert(unit == "player")
    return "Alliance"
end

function GetRealZoneText()
    return "Test Zone"
end

function UnitLevel(unit)
    assert(unit == "player")
    return 17
end

function UnitXP(unit)
    assert(unit == "player")
    return api.xp
end

function UnitXPMax(unit)
    assert(unit == "player")
    return api.max_xp
end

function GetXPExhaustion()
    return 900
end

function GetMoney()
    return api.money
end

function time()
    return 1700000000
end

local frame = { events = {} }

function frame:RegisterEvent(event)
    self.events[event] = true
end

function frame:SetScript(script, handler)
    assert(script == "OnEvent")
    self.on_event = handler
end

function frame:Fire(event, ...)
    if self.events[event] then
        assert(self.on_event, "OnEvent handler was not installed")
        self.on_event(self, event, ...)
    end
end

function CreateFrame(kind)
    assert(kind == "Frame")
    return frame
end

local addon_chunk = assert(loadfile(addon_path))
addon_chunk("WoWCoachCollector")

local key = "11:tbc_classic|10:Test Realm|8:Testhero"

frame:Fire("PLAYER_LOGIN")
local snapshot = assert(WoWCoachCollectorDB.characters[key], "PLAYER_LOGIN did not write a snapshot")
assert(snapshot.xp == 3210, "PLAYER_LOGIN did not capture XP")
assert(snapshot.maxXP == 7800, "PLAYER_LOGIN did not capture maximum XP")
assert(snapshot.moneyCopper == 456789, "PLAYER_LOGIN did not capture money")

api.xp = 0
api.max_xp = 0
api.money = 0
frame:Fire("PLAYER_LOGOUT")
snapshot = WoWCoachCollectorDB.characters[key]
assert(snapshot.xp == 3210, "PLAYER_LOGOUT overwrote valid XP")
assert(snapshot.maxXP == 7800, "PLAYER_LOGOUT overwrote valid maximum XP")
assert(snapshot.moneyCopper == 456789, "PLAYER_LOGOUT overwrote valid money")

api.xp = 4000
api.max_xp = 8000
api.money = 500000
frame:Fire("PLAYER_XP_UPDATE", "target")
snapshot = WoWCoachCollectorDB.characters[key]
assert(snapshot.xp == 3210, "non-player XP update changed the player snapshot")

frame:Fire("PLAYER_XP_UPDATE", "player")
snapshot = WoWCoachCollectorDB.characters[key]
assert(snapshot.xp == 4000, "player XP update did not refresh the snapshot")
assert(snapshot.maxXP == 8000, "player XP update did not refresh maximum XP")
assert(snapshot.moneyCopper == 500000, "player XP update did not refresh money")

print("addon collector tests passed")
