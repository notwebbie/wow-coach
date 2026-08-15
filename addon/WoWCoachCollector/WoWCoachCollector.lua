local ADDON_NAME = ...
local SCHEMA_VERSION = 1

local frame = CreateFrame("Frame")

local function gameFlavor()
    if WOW_PROJECT_ID == WOW_PROJECT_CLASSIC then
        return "classic_era"
    end
    if WOW_PROJECT_ID == WOW_PROJECT_BURNING_CRUSADE_CLASSIC then
        return "tbc_classic"
    end
    if WOW_PROJECT_ID == WOW_PROJECT_MAINLINE then
        return "retail"
    end
    return "anniversary"
end

local function characterKey(flavor, realm, name)
    return string.format("%d:%s|%d:%s|%d:%s", #flavor, flavor, #realm, realm, #name, name)
end

local function collectSnapshot()
    local name = UnitName("player")
    local realm = GetRealmName()
    if not name or name == "" or not realm or realm == "" then
        return
    end

    local flavor = gameFlavor()
    local _, classFile, classID = UnitClass("player")
    local _, raceFile, raceID = UnitRace("player")
    local faction = UnitFactionGroup("player")
    local zone = GetRealZoneText()
    local level = UnitLevel("player")
    local xp = UnitXP("player")
    local maxXP = UnitXPMax("player")
    local restedXP = GetXPExhaustion()

    WoWCoachCollectorDB = WoWCoachCollectorDB or {}
    WoWCoachCollectorDB.schemaVersion = SCHEMA_VERSION
    WoWCoachCollectorDB.gameFlavor = flavor
    WoWCoachCollectorDB.characters = WoWCoachCollectorDB.characters or {}
    WoWCoachCollectorDB.characters[characterKey(flavor, realm, name)] = {
        schemaVersion = SCHEMA_VERSION,
        capturedAt = time(),
        gameFlavor = flavor,
        realm = realm,
        name = name,
        level = level,
        class = classFile,
        classID = classID,
        race = raceFile,
        raceID = raceID,
        faction = faction,
        zone = zone,
        moneyCopper = GetMoney(),
        xp = xp,
        maxXP = maxXP,
        restedXP = restedXP,
    }
end

frame:RegisterEvent("PLAYER_LOGIN")
frame:RegisterEvent("PLAYER_LEVEL_UP")
frame:RegisterEvent("PLAYER_MONEY")
frame:RegisterEvent("PLAYER_XP_UPDATE")
frame:RegisterEvent("ZONE_CHANGED_NEW_AREA")
frame:RegisterEvent("PLAYER_LOGOUT")
frame:SetScript("OnEvent", function(_, event, unit)
    if event ~= "PLAYER_XP_UPDATE" or unit == "player" then
        collectSnapshot()
    end
end)

_G[ADDON_NAME] = { collectSnapshot = collectSnapshot }
