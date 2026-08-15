import Foundation
import XCTest
@testable import WoWCoach

final class WoWCoachTests: XCTestCase {
    func testParsesDataStoreCharacter() throws {
        let lua = #"""
        DataStore_CharactersDB = { ["global"] = { ["Characters"] = {
          ["Default.Example Realm.Examplemage"] = { ["faction"] = "Alliance", ["race"] = "Human", ["class"] = "MAGE", ["level"] = 18, ["money"] = 104800, ["zone"] = "Westfall" },
        } } }
        """#
        let blocks = DataStoreCharacterAnalyzer.characterBlocks(in: lua)
        XCTAssertEqual(blocks.count, 1)
        XCTAssertEqual(DataStoreCharacterAnalyzer.integer("level", in: blocks[0].1), 18)
        XCTAssertEqual(DataStoreCharacterAnalyzer.string("zone", in: blocks[0].1), "Westfall")
    }

    func testDiscoversMultipleAccounts() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root.appendingPathComponent("WTF/Account/EXAMPLE-ONE/SavedVariables"), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: root.appendingPathComponent("WTF/Account/EXAMPLE-TWO/SavedVariables"), withIntermediateDirectories: true)
        let accounts = WoWDiscovery().accounts(in: .init(root: root, flavor: "_anniversary_"))
        XCTAssertEqual(accounts.map(\.name), ["EXAMPLE-ONE", "EXAMPLE-TWO"])
    }

    func testParsesCompactAnniversaryDataStore() throws {
        let characters = #"""
        DataStore_Characters_Info = { { ["name"] = "Examplemage", ["BaseInfo"] = 2894994, ["money"] = 104951, ["XP"] = 8754, ["maxXP"] = 17800, ["restXP"] = 258, ["zone"] = "Westfall", ["subZone"] = "Sentinel Hill", ["bindLocation"] = "Sentinel Hill", }, }
        """#
        let blocks = DataStoreCharacterAnalyzer.arrayRecords(in: characters, table: "DataStore_Characters_Info")
        XCTAssertEqual(blocks.count, 1)
        XCTAssertEqual(DataStoreCharacterAnalyzer.integer("BaseInfo", in: blocks[0])! & 0x7F, 18)
        XCTAssertEqual(DataStoreCharacterAnalyzer.className(for: 9), "WARLOCK")
        XCTAssertEqual(DataStoreCharacterAnalyzer.raceName(for: 5), "Undead")

        let crafts = #"{ ["Indices"] = { ["Herbalism"] = 2, ["Alchemy"] = 1, }, ["Ranks"] = { 9830493, 9830510, }, }"#
        let professions = DataStoreCharacterAnalyzer.professions(in: crafts)
        XCTAssertEqual(professions.first(where: { $0.name == "Alchemy" })?.rank, 93)
        XCTAssertEqual(professions.first(where: { $0.name == "Herbalism" })?.rank, 110)
    }

    func testUsesClassSpecificTalentTreeNames() {
        let talent = #"{ ["PointsSpent"] = "0,13,0", }"#
        XCTAssertEqual(DataStoreCharacterAnalyzer.talentPoints(in: talent, classID: 8), ["Fire": 13])
        XCTAssertEqual(DataStoreCharacterAnalyzer.talentPoints(in: talent, classID: 3), ["Marksmanship": 13])
    }

    func testSummarizesEquippedBagsAndPressure() {
        let containers = #"""
        { ["Containers"] = {
          { ["links"] = { "|cffffffff|Hitem:2589::::::::8::::::::::|h[Linen Cloth]|h|r", }, ["link"] = "|cffffffff|Hitem:10050::::::::8::::::::::|h[Mageweave Bag]|h|r", },
          { ["links"] = { }, ["link"] = "|cff1eff00|Hitem:22246::::::::8::::::::::|h[Enchanted Mageweave Pouch]|h|r", },
        }, }
        """#
        let inventory = DataStoreInventoryAnalyzer.inventory(from: containers)
        XCTAssertEqual(inventory.equippedBags.count, 2)
        XCTAssertEqual(inventory.estimatedTotalSlots, 44)
        XCTAssertEqual(inventory.notableItems.first?.name, "Linen Cloth")
    }
}
