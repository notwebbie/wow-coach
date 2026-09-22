import Foundation
import Testing
@testable import WoWCoach

@Test func parsesDataStoreCharacter() throws {
    let lua = #"""
    DataStore_CharactersDB = { ["global"] = { ["Characters"] = {
      ["Default.Testrealm.Examplelock"] = { ["faction"] = "Horde", ["race"] = "Scourge", ["class"] = "WARLOCK", ["level"] = 18, ["money"] = 104800, ["zone"] = "Silverpine Forest" },
    } } }
    """#
    let blocks = DataStoreCharacterAnalyzer.characterBlocks(in: lua)
    #expect(blocks.count == 1)
    #expect(DataStoreCharacterAnalyzer.integer("level", in: blocks[0].1) == 18)
    #expect(DataStoreCharacterAnalyzer.string("zone", in: blocks[0].1) == "Silverpine Forest")
}

@Test func discoversMultipleAccounts() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root.appendingPathComponent("WTF/Account/ONE/SavedVariables"), withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: root.appendingPathComponent("WTF/Account/TWO/SavedVariables"), withIntermediateDirectories: true)
    let accounts = WoWDiscovery().accounts(in: .init(root: root, flavor: "_anniversary_"))
    #expect(accounts.map(\.name) == ["ONE", "TWO"])
}

@Test func parsesCompactAnniversaryDataStore() throws {
    let characters = #"""
    DataStore_Characters_Info = { { ["name"] = "Examplelock", ["BaseInfo"] = 2894994, ["money"] = 104951, ["XP"] = 8754, ["maxXP"] = 17800, ["restXP"] = 258, ["zone"] = "Silverpine Forest", ["subZone"] = "The Sepulcher", ["bindLocation"] = "The Sepulcher", }, }
    """#
    let blocks = DataStoreCharacterAnalyzer.arrayRecords(in: characters, table: "DataStore_Characters_Info")
    #expect(blocks.count == 1)
    #expect(DataStoreCharacterAnalyzer.integer("BaseInfo", in: blocks[0])! & 0x7F == 18)
    #expect(DataStoreCharacterAnalyzer.className(for: 9) == "WARLOCK")
    #expect(DataStoreCharacterAnalyzer.raceName(for: 5) == "Undead")

    let crafts = #"{ ["Indices"] = { ["Herbalism"] = 2, ["Alchemy"] = 1, }, ["Ranks"] = { 9830493, 9830510, }, }"#
    let professions = DataStoreCharacterAnalyzer.professions(in: crafts)
    #expect(professions.first(where: { $0.name == "Alchemy" })?.rank == 93)
    #expect(professions.first(where: { $0.name == "Herbalism" })?.rank == 110)
}

@Test func usesClassSpecificTalentTreeNames() {
    let talent = #"{ ["PointsSpent"] = "0,13,0", }"#
    #expect(DataStoreCharacterAnalyzer.talentPoints(in: talent, classID: 8) == ["Fire": 13])
    #expect(DataStoreCharacterAnalyzer.talentPoints(in: talent, classID: 3) == ["Marksmanship": 13])
}

@Test func summarizesEquippedBagsAndPressure() {
    let containers = #"""
    { ["bagInfo"] = 10284, ["Containers"] = {
      { ["items"] = { 169672724, }, ["links"] = { "|cffffffff|Hitem:2589::::::::8::::::::::|h[Linen Cloth]|h|r", }, ["link"] = "|cffffffff|Hitem:10050::::::::8::::::::::|h[Mageweave Bag]|h|r", },
      { ["links"] = { }, ["link"] = "|cff1eff00|Hitem:22246::::::::8::::::::::|h[Enchanted Mageweave Pouch]|h|r", },
    }, }
    """#
    let inventory = DataStoreInventoryAnalyzer.inventory(from: containers)
    #expect(inventory.equippedBags.count == 2)
    #expect(inventory.estimatedTotalSlots == 44)
    #expect(inventory.estimatedFreeSlots == 10)
    #expect(inventory.notableItems.first?.count == 20)
    #expect(inventory.notableItems.first?.name == "Linen Cloth")
}

@Test func decodesQuestIdentityLevelAndDifficulty() {
    let questID = 527
    let packedQuest = (questID << 6) + (1 << 1)
    let packedInfo = 24 << 24
    let body = """
    { ["QuestHeaders"] = { "Hillsbrad Foothills", }, ["Quests"] = { \(packedQuest), }, }
    """
    let quests = DataStoreQuestAnalyzer.quests(in: body, titles: [questID: "Battle of Hillsbrad"], infos: [questID: packedInfo], characterLevel: 22)
    #expect(quests.count == 1)
    #expect(quests[0].title == "Battle of Hillsbrad")
    #expect(quests[0].level == 24)
    #expect(quests[0].header == "Hillsbrad Foothills")
    #expect(quests[0].difficulty == "yellow")
    #expect(quests[0].wowheadURL == "https://www.wowhead.com/tbc/quest=527")
}

@Test func parsesQuestMetadataAfterUnicodeText() {
    let text = """
    DataStore_Quests_Characters = { { ["Rewards"] = { "Exampledruid’s reward", }, }, }
    DataStore_Quests_Infos = { [701] = \(37 << 24), }
    DataStore_Quests_Titles = { [701] = "Guile of the Raptor", }
    """
    let metadata = DataStoreQuestAnalyzer.metadata(in: text)
    #expect(metadata.titles[701] == "Guile of the Raptor")
    #expect(metadata.infos[701].map { ($0 >> 24) & 0xFF } == 37)
}

@Test func parsesKnownCraftRecipes() {
    let record = #"{ ["Professions"] = { { ["Name"] = "Alchemy", ["Crafts"] = { "0|Potion", "2|3173", "4|2330", }, }, }, }"#
    let recipes = DataStoreCraftRecipeAnalyzer.recipes(in: record)
    #expect(recipes.count == 2)
    #expect(recipes.first(where: { $0.spellID == 3173 })?.name == "Lesser Mana Potion")
    #expect(recipes.first(where: { $0.spellID == 3173 })?.difficulty == "yellow")
}

@Test func removesCrossProfessionRecipeContamination() {
    let record = #"""
    { ["Professions"] = {
      { ["Name"] = "Enchanting", ["Crafts"] = { "1|7418", "4|2963", "4|2387", }, },
      { ["Name"] = "Tailoring", ["Crafts"] = { "4|2963", "4|2387", }, },
    }, }
    """#
    let recipes = DataStoreCraftRecipeAnalyzer.recipes(in: record)
    #expect(recipes.filter { $0.spellID == 2963 }.count == 1)
    #expect(recipes.first(where: { $0.spellID == 2963 })?.profession == "Tailoring")
    #expect(recipes.first(where: { $0.spellID == 7418 })?.profession == "Enchanting")
}

@Test func recommendsNextConfiguredRotationCharacter() {
    var mage = testCharacter(name: "Examplemage", level: 36, xp: 100, maxXP: 50000)
    var warlock = testCharacter(name: "Examplelock", level: 29, xp: 100, maxXP: 36000)
    mage.lastUpdatedAt = Date(timeIntervalSince1970: 100)
    warlock.lastUpdatedAt = Date(timeIntervalSince1970: 200)
    let profile = CoachingProfile(primaryCharacter: "Examplemage", secondaryCharacter: "Examplelock", bankOnlyCharacters: [],
        objective: nil, targetDate: nil, rotationCharacters: ["Examplemage", "Examplelock"])
    let accounts = [AccountSummary(account: "ONE", sourcePath: "", addons: [], characters: [mage, warlock], findings: [])]
    let progress = [ProgressDelta(character: "Examplelock", levelBefore: 28, levelAfter: 29, xpGained: 1000, moneyChangeCopper: nil, professionChanges: [:])]
    let dashboard = ReportBuilder.dashboard(accounts: accounts, profile: profile, progress: progress, now: Date(timeIntervalSince1970: 300))
    #expect(dashboard.recommendedCharacter == "Examplemage")
}

@Test func estimatesRestedXPForLoggedOutCharacter() {
    var character = testCharacter(name: "Exampledruid", level: 16, xp: 100, maxXP: 15000)
    character.restedXP = 0
    character.isResting = true
    character.lastUpdatedAt = Date(timeIntervalSince1970: 0)
    let estimate = ReportBuilder.estimatedRestedXP(for: character, now: Date(timeIntervalSince1970: 16 * 3600))
    #expect(estimate.value == 1500)
    #expect(estimate.isEstimated)
}

@Test func validatesOptionalSavedVariablesFixture() throws {
    guard let path = ProcessInfo.processInfo.environment["WOWCOACH_FIXTURE"] else { return }
    let directory = URL(fileURLWithPath: path)
    let files = try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
        .filter { $0.pathExtension == "lua" }
    let output = AnalyzerRegistry.standard.run(.init(account: .init(name: "FIXTURE", savedVariables: directory), files: files))
    #expect(!output.characters.isEmpty)
    #expect(output.characters.contains { !($0.activeQuests ?? []).isEmpty })
    #expect(output.characters.contains { !($0.knownRecipes ?? []).isEmpty })
    let quests = output.characters.flatMap { $0.activeQuests ?? [] }
    #expect(quests.allSatisfy { $0.title != nil && $0.level != nil && $0.difficulty != nil })
    let recipes = output.characters.flatMap { $0.knownRecipes ?? [] }
    #expect(recipes.allSatisfy { $0.name != nil })
    #expect(!recipes.contains { $0.profession == "Enchanting" && $0.spellID == 2963 })
    #expect(output.findings.contains { $0.analyzer == "auctionator-scan" && $0.values["market"] != nil })
    #expect(output.characters.allSatisfy { snapshot in
        guard let inventory = snapshot.inventory else { return true }
        return inventory.occupiedSlots <= inventory.estimatedTotalSlots && inventory.estimatedFreeSlots <= inventory.estimatedTotalSlots
    })
}

@Test func calculatesXPWhenCharacterGainsOneLevel() {
    let before = testCharacter(name: "Examplemage", level: 26, xp: 7431, maxXP: 30500)
    let after = testCharacter(name: "Examplemage", level: 27, xp: 20089, maxXP: 32200)
    let previous = ReportSummary(schemaVersion: 3, generatedAt: .distantPast, appVersion: "1.2.0", installationFlavor: "_anniversary_",
        accounts: [.init(account: "ONE", sourcePath: "", addons: [], characters: [before], findings: [])], warnings: [], coachingProfile: nil,
        progressSincePreviousReport: [], previousReportGeneratedAt: nil, coachingDashboard: nil)
    let current = [AccountSummary(account: "ONE", sourcePath: "", addons: [], characters: [after], findings: [])]
    let delta = ReportBuilder.progressDeltas(previous: previous, current: current).first
    #expect(delta?.xpGained == 43158)
}

private func testCharacter(name: String, level: Int, xp: Int, maxXP: Int) -> CharacterSnapshot {
    CharacterSnapshot(name: name, realm: "Testrealm", faction: "Horde", race: "Blood Elf", characterClass: "MAGE",
        level: level, moneyCopper: 0, zone: "Hillsbrad Foothills", subZone: "Tarren Mill", bindLocation: "Freewind Post",
        xp: xp, maxXP: maxXP, restedXP: 0, isResting: true, lastUpdatedAt: .now, professions: [], talentPoints: [:], inventory: nil,
        activeQuests: nil, knownRecipes: nil)
}
