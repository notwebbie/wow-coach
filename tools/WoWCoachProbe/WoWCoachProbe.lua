-- WoW Coach Probe — beta diagnostic, not a shipping addon.
--
-- Answers the questions about the Forever client that reading Blizzard's UI
-- source could not settle. Every probe is wrapped so that a failure is recorded
-- as a result rather than breaking the run: "this errored" is data.
--
-- Usage:
--   /wcprobe            run every probe and print a summary
--   /wcprobe secret     re-run only the Secret Values checks
--   /wcprobe auras      snapshot current player buffs (use at a campsite)
--   /wcprobe recipes    snapshot recipe APIs (run with a profession OPEN, then CLOSED)
--   /wcprobe legacy     dump the Legacy trait trees and hunt for Well Rested
--   /wcprobe pets       hunter pet happiness, loyalty and training points
--   /wcprobe report     print where the results are and how many samples exist

local ADDON = ...
local PROBE_VERSION = 1

local function out(message)
    if DEFAULT_CHAT_FRAME then DEFAULT_CHAT_FRAME:AddMessage("|cff8888ff[wcprobe]|r " .. message) end
end

-- Every probe goes through this. A probe that errors records the error; a probe
-- that returns nothing records that too. Nothing here is allowed to be silent.
local function probe(results, name, fn)
    local ok, a, b, c = pcall(fn)
    if not ok then
        results[name] = { status = "error", detail = tostring(a) }
    elseif a == nil then
        results[name] = { status = "nil" }
    else
        results[name] = { status = "ok", value = a, extra = b, extra2 = c }
    end
    return results[name]
end

local function exists(path)
    local parts, current = {}, _G
    for segment in string.gmatch(path, "[^%.]+") do parts[#parts + 1] = segment end
    for _, segment in ipairs(parts) do
        if type(current) ~= "table" then return false end
        current = current[segment]
        if current == nil then return false end
    end
    return true
end

local function db()
    WoWCoachProbeDB = WoWCoachProbeDB or {}
    WoWCoachProbeDB.probeVersion = PROBE_VERSION
    WoWCoachProbeDB.runs = WoWCoachProbeDB.runs or {}
    WoWCoachProbeDB.restedSamples = WoWCoachProbeDB.restedSamples or {}
    WoWCoachProbeDB.auraSnapshots = WoWCoachProbeDB.auraSnapshots or {}
    WoWCoachProbeDB.recipeSnapshots = WoWCoachProbeDB.recipeSnapshots or {}
    return WoWCoachProbeDB
end

--------------------------------------------------------------------------------
-- 1. Secret Values
--
-- The highest-stakes unknown. On this client many fields are typed `string`
-- rather than `cstring`, which in retail's Secret Values system means the value
-- may not be readable, comparable, concatenable, or storable while tainted.
-- If quest titles are secret, the whole quest capture path is dead as designed.
--
-- The test is deliberately staged: type, compare, concatenate, store. Each stage
-- is recorded separately so we learn WHICH operation is blocked, not just that
-- something failed. The stored value is checked again after a reload, because a
-- secret may write successfully and come back as something else.
--------------------------------------------------------------------------------

local function probeSecretValues(results)
    local sample
    probe(results, "questlog.getInfo", function()
        if not exists("C_QuestLog.GetNumQuestLogEntries") then return nil end
        local shown = C_QuestLog.GetNumQuestLogEntries()
        for index = 1, (shown or 0) do
            local info = C_QuestLog.GetInfo(index)
            if info and not info.isHeader then sample = info return "found a quest" end
        end
        return nil
    end)

    if not sample then
        results["secret.note"] = { status = "skipped", detail = "no non-header quest in the log" }
        return
    end

    probe(results, "secret.type", function() return type(sample.title) end)
    probe(results, "secret.tostring", function() return tostring(sample.title) end)
    probe(results, "secret.compare", function() return (sample.title == "") and "compared" or "compared" end)
    probe(results, "secret.concat", function() return "prefix:" .. sample.title end)
    probe(results, "secret.len", function() return #sample.title end)
    probe(results, "secret.format", function() return string.format("%s", sample.title) end)

    -- Store it. If it survives a reload intact, titles are safe to capture.
    probe(results, "secret.store", function()
        db().secretProbe = {
            writtenAt = time(),
            questID = sample.questID,
            title = sample.title,
            titleType = type(sample.title),
        }
        return "stored"
    end)

    -- Same questions for the other string fields we would capture.
    probe(results, "secret.skillName", function()
        if not exists("C_SkillInfo.GetSkillLineInfo") then return nil end
        local info = C_SkillInfo.GetSkillLineInfo(1)
        if not info then return nil end
        return type(info.name) .. " / " .. tostring("concat:" .. info.name)
    end)
    probe(results, "secret.zone", function() return type(GetRealZoneText()) end)
    probe(results, "secret.unitName", function() return type(UnitName("player")) end)
end

-- Read back what the previous session stored. This is the actual answer to the
-- Secret Values question: a write that appeared to succeed can still come back
-- as a placeholder, an empty string, or a missing key.
local function checkStoredSecret(results)
    local stored = db().secretProbe
    if not stored then
        results["secret.reload"] = { status = "skipped", detail = "no prior run to compare" }
        return
    end
    results["secret.reload"] = {
        status = "ok",
        value = string.format("wrote type=%s, read back type=%s, value=%q",
            tostring(stored.titleType), type(stored.title), tostring(stored.title)),
    }
end

--------------------------------------------------------------------------------
-- 2. Ruleset
--
-- Hardcore is confirmed in Blizzard's own code. PvP and RP exist in the enum
-- but nothing in the shipped UI calls them, so this is the only way to find out.
--------------------------------------------------------------------------------

local function probeRuleset(results)
    probe(results, "ruleset.hardcore", function()
        if not exists("C_GameRules.IsHardcoreActive") then return nil end
        return tostring(C_GameRules.IsHardcoreActive())
    end)
    probe(results, "ruleset.selfFound", function()
        if not exists("C_GameRules.IsSelfFoundAllowed") then return nil end
        return tostring(C_GameRules.IsSelfFoundAllowed())
    end)
    for _, rule in ipairs({ "HardcoreRuleset", "PvPRuleset", "RPRuleset", "SelfFoundAllowed" }) do
        probe(results, "ruleset.isActive." .. rule, function()
            if not exists("C_GameRules.IsGameRuleActive") then return nil end
            if not Enum or not Enum.GameRule or Enum.GameRule[rule] == nil then
                return "enum member missing"
            end
            return tostring(C_GameRules.IsGameRuleActive(Enum.GameRule[rule]))
                .. " (enum=" .. tostring(Enum.GameRule[rule]) .. ")"
        end)
    end
    probe(results, "ruleset.realmName", function() return GetRealmName() end)
    probe(results, "ruleset.normalizedRealm", function()
        if not GetNormalizedRealmName then return nil end
        return GetNormalizedRealmName()
    end)
    probe(results, "ruleset.gameMode", function()
        if not exists("C_GameRules.GetActiveGameMode") then return nil end
        return tostring(C_GameRules.GetActiveGameMode())
    end)
end

--------------------------------------------------------------------------------
-- 3. Professions and skills
--------------------------------------------------------------------------------

local function probeSkills(results)
    probe(results, "skills.namespace", function() return tostring(exists("C_SkillInfo")) end)
    probe(results, "skills.count", function()
        if not exists("C_SkillInfo.GetNumSkillLines") then return nil end
        return C_SkillInfo.GetNumSkillLines()
    end)
    probe(results, "skills.firstLine", function()
        if not exists("C_SkillInfo.GetSkillLineInfo") then return nil end
        local info = C_SkillInfo.GetSkillLineInfo(1)
        if not info then return nil end
        local keys = {}
        for key in pairs(info) do keys[#keys + 1] = key end
        table.sort(keys)
        return table.concat(keys, ",")
    end)
    -- Does it work before the Skills panel has ever been opened? That decides
    -- whether a snapshot at login is trustworthy.
    probe(results, "skills.byID.defense95", function()
        if not exists("C_SkillInfo.GetSkillLineInfoByID") then return nil end
        local info = C_SkillInfo.GetSkillLineInfoByID(95)
        if not info then return nil end
        return string.format("rank=%s max=%s collapsed=%s", tostring(info.rank),
            tostring(info.maxRank), tostring(info.isCollapsed))
    end)
    probe(results, "professions.arity", function()
        if not GetProfessions then return nil end
        local values = { GetProfessions() }
        return "returned " .. #values .. " values: " .. table.concat(
            (function() local t = {} for i = 1, 10 do t[i] = tostring(values[i]) end return t end)(), ",")
    end)
    probe(results, "professions.slots", function()
        if not GetProfessions or not GetProfessionInfo then return nil end
        local parts = {}
        for _, index in ipairs({ GetProfessions() }) do
            local name, _, rank, maxRank, _, _, skillLine = GetProfessionInfo(index)
            parts[#parts + 1] = string.format("[%s]%s rank=%s/%s line=%s", tostring(index),
                tostring(name), tostring(rank), tostring(maxRank), tostring(skillLine))
        end
        return table.concat(parts, " | ")
    end)
end

--------------------------------------------------------------------------------
-- 4. Talents
--------------------------------------------------------------------------------

local function probeTalents(results)
    probe(results, "talents.configID", function()
        if not exists("C_SpecializationInfo.GetCombatConfigIDForSpecGroup") then return nil end
        return tostring(C_SpecializationInfo.GetCombatConfigIDForSpecGroup(1))
    end)
    probe(results, "talents.activeConfigID", function()
        if not exists("C_ClassTalents.GetActiveConfigID") then return nil end
        return tostring(C_ClassTalents.GetActiveConfigID())
    end)
    probe(results, "talents.hasUnspent", function()
        if not exists("C_ClassTalents.HasUnspentTalentPoints") then return nil end
        return tostring(C_ClassTalents.HasUnspentTalentPoints())
    end)
    probe(results, "talents.groups", function()
        if not exists("C_SpecializationInfo.GetCombatConfigIDForSpecGroup") then return nil end
        if not exists("C_Traits.GetGroupDisplayInfoByTreeID") then return nil end
        local configID = C_SpecializationInfo.GetCombatConfigIDForSpecGroup(1)
        if not configID then return nil end
        local specID = exists("C_SpecializationInfo.GetSpecialization")
            and C_SpecializationInfo.GetSpecialization() or nil
        local treeID = specID and exists("C_ClassTalents.GetTraitTreeForSpec")
            and C_ClassTalents.GetTraitTreeForSpec(specID) or nil
        if not treeID then return "could not resolve treeID (specID=" .. tostring(specID) .. ")" end
        local displays = C_Traits.GetGroupDisplayInfoByTreeID(treeID)
        if not displays then return "no display info for tree " .. tostring(treeID) end
        local ids, names = {}, {}
        for _, display in ipairs(displays) do
            ids[#ids + 1] = display.groupID
            names[#names + 1] = tostring(display.displayName)
        end
        local currencies = C_Traits.GetGroupCurrencyInfo(configID, ids)
        local parts = {}
        for index, group in ipairs(currencies or {}) do
            local first = group.currencyInfos and group.currencyInfos[1]
            parts[#parts + 1] = string.format("%s spent=%s",
                names[index] or "?", first and tostring(first.spent) or "?")
        end
        return "tree=" .. tostring(treeID) .. " " .. table.concat(parts, " | ")
    end)
end

--------------------------------------------------------------------------------
-- 5. Legacy System
--
-- Tree and currency IDs are constants in Blizzard's own code. The open question
-- is whether they read outside the Legacy UI, and which node is Well Rested —
-- which matters because that perk changes rested accrual and the rested cap,
-- the numbers the coaching engine depends on.
--------------------------------------------------------------------------------

local LEGACY_TREES = { professions = 1187, adventure = 1188, resourcefulness = 1189 }

local function probeLegacy(results, verbose)
    for label, treeID in pairs(LEGACY_TREES) do
        probe(results, "legacy." .. label, function()
            if not exists("C_Traits.GetConfigIDByTreeID") then return nil end
            local configID = C_Traits.GetConfigIDByTreeID(treeID)
            if not configID then return "no configID (tree may need the Legacy UI opened once)" end
            local info = C_Traits.GetTreeCurrencyInfo(configID, treeID, true)
            local first = info and info[1]
            if not first then return "configID=" .. tostring(configID) .. " but no currency info" end
            return string.format("configID=%s available=%s cap=%s spentInTree=%s",
                tostring(configID), tostring(first.quantity), tostring(first.maxQuantity),
                tostring(first.spentInTree))
        end)
    end

    probe(results, "legacy.renown", function()
        if not exists("C_MajorFactions.GetCurrentRenownLevel") then return nil end
        return tostring(C_MajorFactions.GetCurrentRenownLevel(2802))
    end)

    if not verbose then return end

    -- Dump every node in the Adventure tree so we can identify Well Rested by
    -- name and record its nodeID for the collector to read directly.
    probe(results, "legacy.adventureNodes", function()
        if not exists("C_Traits.GetTreeNodes") or not exists("C_Traits.GetNodeInfo") then return nil end
        local configID = C_Traits.GetConfigIDByTreeID(LEGACY_TREES.adventure)
        if not configID then return "no configID" end
        local nodes = C_Traits.GetTreeNodes(LEGACY_TREES.adventure)
        local dump = {}
        for _, nodeID in ipairs(nodes or {}) do
            local node = C_Traits.GetNodeInfo(configID, nodeID)
            if node then
                local name
                if node.entryIDs and node.entryIDs[1] and exists("C_Traits.GetEntryInfo") then
                    local entry = C_Traits.GetEntryInfo(configID, node.entryIDs[1])
                    local definition = entry and entry.definitionID
                        and exists("C_Traits.GetDefinitionInfo")
                        and C_Traits.GetDefinitionInfo(entry.definitionID)
                    name = definition and definition.overrideName
                end
                dump[#dump + 1] = {
                    nodeID = nodeID,
                    name = tostring(name),
                    ranksPurchased = node.ranksPurchased,
                    maxRanks = node.maxRanks,
                }
            end
        end
        db().legacyAdventureNodes = dump
        return #dump .. " nodes dumped to SavedVariables"
    end)
end

--------------------------------------------------------------------------------
-- 6. Recipes
--
-- Run once with a profession window OPEN and once with it CLOSED. The delta is
-- the answer to whether the window constraint still applies.
--------------------------------------------------------------------------------

local function probeRecipes(results, label)
    local snapshot = { at = time(), label = label or "unlabelled" }

    probe(results, "recipes.ready", function()
        if not exists("C_TradeSkillUI.IsTradeSkillReady") then return nil end
        return tostring(C_TradeSkillUI.IsTradeSkillReady())
    end)
    probe(results, "recipes.baseProfession", function()
        if not exists("C_TradeSkillUI.GetBaseProfessionInfo") then return nil end
        local info = C_TradeSkillUI.GetBaseProfessionInfo()
        if not info then return nil end
        return string.format("%s skill=%s/%s id=%s", tostring(info.professionName),
            tostring(info.skillLevel), tostring(info.maxSkillLevel), tostring(info.professionID))
    end)
    probe(results, "recipes.allRecipeIDs", function()
        if not exists("C_TradeSkillUI.GetAllRecipeIDs") then return nil end
        local ids = C_TradeSkillUI.GetAllRecipeIDs()
        if type(ids) ~= "table" then return "returned " .. type(ids) end
        snapshot.count = #ids
        snapshot.sample = {}
        for index = 1, math.min(#ids, 5) do snapshot.sample[index] = ids[index] end
        return #ids .. " recipe ids"
    end)
    probe(results, "recipes.recipeInfo", function()
        if not exists("C_TradeSkillUI.GetAllRecipeIDs") or not exists("C_TradeSkillUI.GetRecipeInfo") then
            return nil
        end
        local ids = C_TradeSkillUI.GetAllRecipeIDs()
        if type(ids) ~= "table" or not ids[1] then return "no recipe ids to inspect" end
        local info = C_TradeSkillUI.GetRecipeInfo(ids[1])
        if type(info) ~= "table" then return "returned " .. type(info) end
        local keys = {}
        for key in pairs(info) do keys[#keys + 1] = key end
        table.sort(keys)
        snapshot.recipeInfoKeys = keys
        return table.concat(keys, ",")
    end)

    local store = db().recipeSnapshots
    store[#store + 1] = snapshot
end

--------------------------------------------------------------------------------
-- 7. Rested XP sampling
--
-- Nothing in the API exposes the accrual rate or the cap, and the Legacy
-- "Well Rested" perk changes both. The only way to know the real numbers is to
-- measure them, so every exhaustion change is logged with a timestamp and the
-- rate is fitted offline.
--------------------------------------------------------------------------------

local function sampleRested(reason)
    -- `x and x() or nil` collapses a false or zero result to nil, which drops
    -- the key entirely and makes "not rested" indistinguishable from "no such
    -- API". The rested-rate fit needs the zeros and the falses, so each value
    -- is captured with an explicit presence check.
    local exhaustion, resting, restState
    if GetXPExhaustion then exhaustion = GetXPExhaustion() or 0 end
    if IsResting then resting = IsResting() and true or false end
    if GetRestState then restState = GetRestState() end

    local store = db().restedSamples
    store[#store + 1] = {
        at = time(),
        reason = reason,
        exhaustion = exhaustion,
        xp = UnitXP and UnitXP("player") or nil,
        maxXP = UnitXPMax and UnitXPMax("player") or nil,
        level = UnitLevel and UnitLevel("player") or nil,
        isResting = resting,
        restState = restState,
        zone = GetRealZoneText and GetRealZoneText() or nil,
    }
end

--------------------------------------------------------------------------------
-- 8. Campsite auras
--
-- Camping has no API at all. The only handle is the buff it applies, so this
-- records every player aura for matching by hand afterwards.
--------------------------------------------------------------------------------

local function snapshotAuras(label)
    local auras = {}
    if exists("C_UnitAuras.GetAuraDataByIndex") then
        for index = 1, 40 do
            local data = C_UnitAuras.GetAuraDataByIndex("player", index, "HELPFUL")
            if not data then break end
            auras[#auras + 1] = {
                spellID = data.spellId,
                name = tostring(data.name),
                source = tostring(data.sourceUnit),
            }
        end
    end
    local store = db().auraSnapshots
    store[#store + 1] = { at = time(), label = label or "unlabelled", auras = auras }
    return #auras
end

--------------------------------------------------------------------------------
-- 8b. Hunter pet
--
-- Happiness, loyalty and training points are Forever-only C_PetInfo functions
-- that ALSO have TBC equivalents as bare globals. That makes them genuine
-- shared-schema fields rather than a one-flavor curiosity, and no other class
-- can exercise this path. Run on a hunter with a pet out.
--------------------------------------------------------------------------------

local function probePets(results)
    probe(results, "pet.exists", function()
        if not UnitExists then return nil end
        return tostring(UnitExists("pet")) .. " name=" ..
            tostring(UnitName and UnitName("pet") or "?")
    end)
    probe(results, "pet.namespace", function() return tostring(exists("C_PetInfo")) end)
    probe(results, "pet.happiness", function()
        if exists("C_PetInfo.GetPetHappiness") then
            local happiness, damage, loyalty = C_PetInfo.GetPetHappiness()
            return string.format("C_PetInfo happiness=%s damage=%s loyalty=%s",
                tostring(happiness), tostring(damage), tostring(loyalty))
        end
        if GetPetHappiness then
            local happiness, damage, loyalty = GetPetHappiness()
            return string.format("global happiness=%s damage=%s loyalty=%s",
                tostring(happiness), tostring(damage), tostring(loyalty))
        end
        return nil
    end)
    probe(results, "pet.loyalty", function()
        if exists("C_PetInfo.GetPetLoyalty") then return tostring(C_PetInfo.GetPetLoyalty()) end
        return nil
    end)
    probe(results, "pet.trainingPoints", function()
        if exists("C_PetInfo.GetPetTrainingPoints") then
            local spent, total = C_PetInfo.GetPetTrainingPoints()
            return string.format("C_PetInfo spent=%s total=%s", tostring(spent), tostring(total))
        end
        if GetPetTrainingPoints then
            local spent, total = GetPetTrainingPoints()
            return string.format("global spent=%s total=%s", tostring(spent), tostring(total))
        end
        return nil
    end)
    probe(results, "pet.foodTypes", function()
        if not exists("C_PetInfo.GetPetFoodTypes") then return nil end
        local types = C_PetInfo.GetPetFoodTypes()
        if type(types) ~= "table" then return "returned " .. type(types) end
        return table.concat(types, ",")
    end)
    probe(results, "pet.stableSlots", function()
        if not exists("C_StableInfo.GetNumStableSlots") then return nil end
        return tostring(C_StableInfo.GetNumStableSlots())
    end)
end

--------------------------------------------------------------------------------
-- Run
--------------------------------------------------------------------------------

local function runAll()
    local results = {}
    local version, build, date, interface = GetBuildInfo()
    results["client"] = { status = "ok",
        value = string.format("%s build=%s date=%s interface=%s", tostring(version),
            tostring(build), tostring(date), tostring(interface)) }
    results["client.project"] = { status = "ok", value = tostring(WOW_PROJECT_ID) }
    -- The proposed flavor sentinel: a namespace that exists on no other client.
    results["client.hasSkillInfo"] = { status = "ok", value = tostring(exists("C_SkillInfo")) }

    checkStoredSecret(results)
    probeSecretValues(results)
    probeRuleset(results)
    probeSkills(results)
    probeTalents(results)
    probeLegacy(results, false)
    probePets(results)
    probeRecipes(results, "runAll")
    sampleRested("runAll")

    local run = { at = time(), interface = interface, results = results }
    local runs = db().runs
    runs[#runs + 1] = run

    local count = 0
    for name, result in pairs(results) do
        count = count + 1
        if result.status ~= "ok" then
            out(string.format("|cffff8800%s|r: %s %s", name, result.status, tostring(result.detail or "")))
        end
    end
    out(string.format("ran %d probes; full results in SavedVariables. Notable answers:", count))
    for _, key in ipairs({ "secret.type", "secret.concat", "secret.reload",
                           "ruleset.isActive.PvPRuleset", "skills.byID.defense95",
                           "recipes.allRecipeIDs", "legacy.adventure", "pet.happiness" }) do
        local result = results[key]
        if result then
            out(string.format("  %s = %s", key, tostring(result.value or result.detail or result.status)))
        end
    end
    out("now /reload and run |cffffff00/wcprobe|r again to complete the Secret Values check.")
end

local frame = CreateFrame("Frame")
frame:RegisterEvent("PLAYER_LOGIN")
frame:RegisterEvent("UPDATE_EXHAUSTION")
frame:RegisterEvent("PLAYER_UPDATE_RESTING")
frame:RegisterEvent("PLAYER_XP_UPDATE")
frame:SetScript("OnEvent", function(_, event)
    if event == "PLAYER_LOGIN" then
        sampleRested("login")
        out("loaded. Run |cffffff00/wcprobe|r. Rested XP is being sampled automatically.")
        return
    end
    sampleRested(event)
end)

SLASH_WCPROBE1 = "/wcprobe"
SlashCmdList["WCPROBE"] = function(argument)
    local command = string.lower(string.match(argument or "", "^%s*(%S*)") or "")
    if command == "auras" then
        out(snapshotAuras("manual") .. " auras recorded. Do this at a campsite and away from one.")
    elseif command == "recipes" then
        local results = {}
        probeRecipes(results, "manual")
        for name, result in pairs(results) do
            out(string.format("  %s = %s", name, tostring(result.value or result.detail or result.status)))
        end
    elseif command == "legacy" then
        local results = {}
        probeLegacy(results, true)
        for name, result in pairs(results) do
            out(string.format("  %s = %s", name, tostring(result.value or result.detail or result.status)))
        end
    elseif command == "pets" then
        local results = {}
        probePets(results)
        for name, result in pairs(results) do
            out(string.format("  %s = %s", name, tostring(result.value or result.detail or result.status)))
        end
    elseif command == "secret" then
        local results = {}
        checkStoredSecret(results)
        probeSecretValues(results)
        for name, result in pairs(results) do
            out(string.format("  %s = %s", name, tostring(result.value or result.detail or result.status)))
        end
    elseif command == "report" then
        local data = db()
        out(string.format("%d runs, %d rested samples, %d aura snapshots, %d recipe snapshots",
            #data.runs, #data.restedSamples, #data.auraSnapshots, #data.recipeSnapshots))
    else
        runAll()
    end
end
