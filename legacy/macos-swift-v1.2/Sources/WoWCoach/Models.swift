import Foundation

struct WoWInstallation: Identifiable, Hashable, Codable, Sendable {
    let root: URL
    let flavor: String
    var id: String { root.path }
}

struct AccountSource: Identifiable, Hashable, Codable, Sendable {
    let name: String
    let savedVariables: URL
    var id: String { savedVariables.path }
}

struct CharacterSnapshot: Codable, Hashable, Sendable {
    var name: String
    var realm: String?
    var faction: String?
    var race: String?
    var characterClass: String?
    var level: Int?
    var moneyCopper: Int?
    var zone: String?
    var subZone: String?
    var bindLocation: String?
    var xp: Int?
    var maxXP: Int?
    var restedXP: Int?
    var isResting: Bool?
    var lastUpdatedAt: Date?
    var professions: [ProfessionSnapshot]
    var talentPoints: [String: Int]
    var inventory: InventorySnapshot?
}

struct InventorySnapshot: Codable, Hashable, Sendable {
    let equippedBags: [BagSnapshot]
    let estimatedTotalSlots: Int
    let occupiedSlots: Int
    let estimatedFreeSlots: Int
    let bagPressure: String
    let notableItems: [InventoryItemSnapshot]
}

struct BagSnapshot: Codable, Hashable, Sendable {
    let itemID: Int
    let name: String
    let slots: Int?
}

struct InventoryItemSnapshot: Codable, Hashable, Sendable {
    let itemID: Int
    let name: String
    let count: Int
}

struct ProfessionSnapshot: Codable, Hashable, Sendable {
    let name: String
    let rank: Int
    let maximumRank: Int
}

struct AddonSnapshot: Codable, Hashable, Sendable {
    let name: String
    let files: [String]
    let byteCount: Int
}

struct AnalyzerFinding: Codable, Hashable, Sendable {
    let analyzer: String
    let values: [String: String]
}

struct AccountSummary: Codable, Sendable {
    let account: String
    let sourcePath: String
    let addons: [AddonSnapshot]
    let characters: [CharacterSnapshot]
    let findings: [AnalyzerFinding]
}

struct CoachingProfile: Codable, Sendable {
    let primaryCharacter: String?
    let secondaryCharacter: String?
    let bankOnlyCharacters: [String]
    let objective: String?
    let targetDate: String?
}

struct ProgressDelta: Codable, Hashable, Sendable {
    let character: String
    let levelBefore: Int?
    let levelAfter: Int?
    let xpGained: Int?
    let moneyChangeCopper: Int?
    let professionChanges: [String: Int]
}

struct ReportSummary: Codable, Sendable {
    let schemaVersion: Int
    let generatedAt: Date
    let appVersion: String
    let installationFlavor: String
    let accounts: [AccountSummary]
    let warnings: [String]
    let coachingProfile: CoachingProfile?
    let progressSincePreviousReport: [ProgressDelta]
}

struct ReportResult: Sendable {
    let zipURL: URL
    let accountCount: Int
    let fileCount: Int
}
