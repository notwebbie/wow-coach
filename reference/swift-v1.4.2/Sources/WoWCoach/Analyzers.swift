import Foundation

struct AnalyzerContext: Sendable {
    let account: AccountSource
    let files: [URL]
}

protocol SavedVariablesAnalyzer: Sendable {
    var identifier: String { get }
    func analyze(_ context: AnalyzerContext) throws -> AnalyzerOutput
}

struct AnalyzerOutput: Sendable {
    var characters: [CharacterSnapshot] = []
    var findings: [AnalyzerFinding] = []
    var warnings: [String] = []
}

struct AnalyzerRegistry: Sendable {
    let analyzers: [any SavedVariablesAnalyzer]

    static let standard = AnalyzerRegistry(analyzers: [
        DataStoreCharacterAnalyzer(), DataStoreInventoryAnalyzer(), DataStoreQuestAnalyzer(),
        DataStoreCraftRecipeAnalyzer(), AuctionatorScanAnalyzer(), AddonMetadataAnalyzer()
    ])

    func run(_ context: AnalyzerContext) -> AnalyzerOutput {
        var result = AnalyzerOutput()
        for analyzer in analyzers {
            do {
                let next = try analyzer.analyze(context)
                result.characters.append(contentsOf: next.characters)
                result.findings.append(contentsOf: next.findings)
                result.warnings.append(contentsOf: next.warnings)
            } catch {
                result.warnings.append("\(analyzer.identifier): \(error.localizedDescription)")
            }
        }
        result.characters = result.characters.reduce(into: []) { merged, character in
            if let index = merged.firstIndex(where: { $0.name == character.name }) {
                if merged[index].inventory == nil { merged[index].inventory = character.inventory }
                if merged[index].activeQuests == nil { merged[index].activeQuests = character.activeQuests }
                if merged[index].knownRecipes == nil { merged[index].knownRecipes = character.knownRecipes }
            } else { merged.append(character) }
        }.sorted { $0.name < $1.name }
        return result
    }
}

struct AddonMetadataAnalyzer: SavedVariablesAnalyzer {
    let identifier = "addon-metadata"
    func analyze(_ context: AnalyzerContext) throws -> AnalyzerOutput {
        let names = Set(context.files.map { $0.deletingPathExtension().lastPathComponent })
        return AnalyzerOutput(findings: [.init(analyzer: identifier, values: [
            "addonFileCount": String(context.files.count),
            "detectedAddons": names.sorted().joined(separator: ", ")
        ])])
    }
}

struct DataStoreCharacterAnalyzer: SavedVariablesAnalyzer {
    let identifier = "datastore-characters"

    func analyze(_ context: AnalyzerContext) throws -> AnalyzerOutput {
        guard let file = context.files.first(where: { $0.lastPathComponent.caseInsensitiveCompare("DataStore_Characters.lua") == .orderedSame }) else { return AnalyzerOutput() }
        let text = String(decoding: try Data(contentsOf: file), as: UTF8.self)
        let blocks = Self.characterBlocks(in: text)
        var characters = blocks.compactMap { key, body -> CharacterSnapshot? in
            let parts = key.split(separator: ".").map(String.init)
            guard let name = parts.last, !name.isEmpty else { return nil }
            return CharacterSnapshot(
                name: name,
                realm: parts.count >= 2 ? parts[parts.count - 2] : nil,
                faction: Self.string("faction", in: body),
                race: Self.string("race", in: body),
                characterClass: Self.string("class", in: body),
                level: Self.integer("level", in: body),
                moneyCopper: Self.integer("money", in: body),
                zone: Self.string("zone", in: body),
                subZone: Self.string("subZone", in: body),
                bindLocation: Self.string("bindLocation", in: body),
                xp: Self.integer("XP", in: body),
                maxXP: Self.integer("maxXP", in: body),
                restedXP: Self.integer("restXP", in: body),
                isResting: nil,
                lastUpdatedAt: Self.timestamp("lastUpdate", in: body),
                professions: [],
                talentPoints: [:],
                inventory: nil,
                activeQuests: nil,
                knownRecipes: nil
            )
        }
        if characters.isEmpty {
            characters = try compactCharacters(characterText: text, context: context)
        }
        return AnalyzerOutput(characters: characters)
    }

    private func compactCharacters(characterText: String, context: AnalyzerContext) throws -> [CharacterSnapshot] {
        let characterBlocks = Self.arrayRecords(in: characterText, table: "DataStore_Characters_Info")
        let craftBlocks = try Self.records(from: context, file: "DataStore_Crafts.lua", table: "DataStore_Crafts_Characters")
        let talentBlocks = try Self.records(from: context, file: "DataStore_Talents.lua", table: "DataStore_Talents_Characters")

        return characterBlocks.enumerated().compactMap { index, body in
            guard let name = Self.string("name", in: body) else { return nil }
            let baseInfo = Self.integer("BaseInfo", in: body) ?? 0
            let classID = (baseInfo >> 7) & 0xF
            let raceID = (baseInfo >> 11) & 0x7F
            let craft = index < craftBlocks.count ? craftBlocks[index] : ""
            let talent = index < talentBlocks.count ? talentBlocks[index] : ""
            return CharacterSnapshot(
                name: name,
                realm: nil,
                faction: Self.faction(forRaceID: raceID),
                race: Self.raceName(for: raceID),
                characterClass: Self.className(for: classID) ?? Self.string("Class", in: talent),
                level: baseInfo & 0x7F,
                moneyCopper: Self.integer("money", in: body),
                zone: Self.string("zone", in: body),
                subZone: Self.string("subZone", in: body),
                bindLocation: Self.string("bindLocation", in: body),
                xp: Self.integer("XP", in: body),
                maxXP: Self.integer("maxXP", in: body),
                restedXP: Self.integer("restXP", in: body),
                isResting: ((baseInfo >> 21) & 1) == 1,
                lastUpdatedAt: Self.timestamp("lastUpdate", in: body),
                professions: Self.professions(in: craft),
                talentPoints: Self.talentPoints(in: talent, classID: classID),
                inventory: nil,
                activeQuests: nil,
                knownRecipes: nil
            )
        }
    }

    private static func records(from context: AnalyzerContext, file: String, table: String) throws -> [String] {
        guard let url = context.files.first(where: { $0.lastPathComponent == file }) else { return [] }
        return arrayRecords(in: try String(contentsOf: url, encoding: .utf8), table: table)
    }

    static func arrayRecords(in text: String, table: String) -> [String] {
        guard let nameRange = text.range(of: table), let equals = text[nameRange.upperBound...].firstIndex(of: "="), let open = text[equals...].firstIndex(of: "{") else { return [] }
        let ns = text as NSString
        let start = NSRange(text.startIndex..<open, in: text).length
        var records: [String] = [], depth = 0, recordStart: Int?
        var quoted = false, escaped = false, index = start
        while index < ns.length {
            let char = ns.character(at: index)
            if escaped { escaped = false }
            else if char == 92 && quoted { escaped = true }
            else if char == 34 { quoted.toggle() }
            else if !quoted && char == 123 {
                depth += 1
                if depth == 2 { recordStart = index }
            } else if !quoted && char == 125 {
                if depth == 2, let startOfRecord = recordStart {
                    records.append(ns.substring(with: NSRange(location: startOfRecord, length: index - startOfRecord + 1)))
                    recordStart = nil
                }
                depth -= 1
                if depth == 0 { break }
            }
            index += 1
        }
        return records
    }

    static func professions(in body: String) -> [ProfessionSnapshot] {
        guard !body.isEmpty else { return [] }
        let indicesBody = nestedTable("Indices", in: body) ?? ""
        let ranksBody = nestedTable("Ranks", in: body) ?? ""
        let names = keyValueStrings(in: indicesBody)
        return names.compactMap { name, indexText in
            guard let index = Int(indexText), let packed = indexedInteger(index, in: ranksBody) else { return nil }
            return ProfessionSnapshot(name: name, rank: packed & 0xFFFF, maximumRank: packed >> 16)
        }.sorted { $0.name < $1.name }
    }

    static func talentPoints(in body: String, classID: Int) -> [String: Int] {
        guard let points = string("PointsSpent", in: body) else { return [:] }
        let values = points.split(separator: ",").compactMap { Int($0) }
        let names = talentTreeNames[classID] ?? ["Tree 1", "Tree 2", "Tree 3"]
        return Dictionary(uniqueKeysWithValues: zip(names, values).filter { $0.1 > 0 })
    }

    static let talentTreeNames: [Int: [String]] = [
        1: ["Arms", "Fury", "Protection"], 2: ["Holy", "Protection", "Retribution"],
        3: ["Beast Mastery", "Marksmanship", "Survival"], 4: ["Assassination", "Combat", "Subtlety"],
        5: ["Discipline", "Holy", "Shadow"], 7: ["Elemental", "Enhancement", "Restoration"],
        8: ["Arcane", "Fire", "Frost"], 9: ["Affliction", "Demonology", "Destruction"],
        11: ["Balance", "Feral Combat", "Restoration"]
    ]

    static func nestedTable(_ key: String, in body: String) -> String? {
        guard let regex = try? NSRegularExpression(pattern: #"\[\""# + NSRegularExpression.escapedPattern(for: key) + #"\"\]\s*=\s*\{"#), let match = regex.firstMatch(in: body, range: NSRange(body.startIndex..., in: body)) else { return nil }
        let ns = body as NSString
        var depth = 1, index = match.range.location + match.range.length, quoted = false, escaped = false
        while index < ns.length && depth > 0 {
            let char = ns.character(at: index)
            if escaped { escaped = false }
            else if char == 92 && quoted { escaped = true }
            else if char == 34 { quoted.toggle() }
            else if !quoted && char == 123 { depth += 1 }
            else if !quoted && char == 125 { depth -= 1 }
            index += 1
        }
        return ns.substring(with: NSRange(location: match.range.location, length: index - match.range.location))
    }

    static func keyValueStrings(in body: String) -> [(String, String)] {
        guard let regex = try? NSRegularExpression(pattern: #"\[\"([^\"]+)\"\]\s*=\s*(\d+)"#) else { return [] }
        return regex.matches(in: body, range: NSRange(body.startIndex..., in: body)).compactMap { match in
            guard let key = Range(match.range(at: 1), in: body), let value = Range(match.range(at: 2), in: body) else { return nil }
            return (String(body[key]), String(body[value]))
        }
    }

    static func indexedInteger(_ index: Int, in body: String) -> Int? {
        if let explicit = capture(#"\["# + String(index) + #"\]\s*=\s*(\d+)"#, in: body) { return Int(explicit) }
        guard let regex = try? NSRegularExpression(pattern: #"(\d+)\s*,"#) else { return nil }
        let implicit = regex.matches(in: body, range: NSRange(body.startIndex..., in: body)).compactMap { match -> Int? in
            guard let range = Range(match.range(at: 1), in: body) else { return nil }
            return Int(body[range])
        }
        return index > 0 && index <= implicit.count ? implicit[index - 1] : nil
    }

    static func className(for id: Int) -> String? {
        // Class IDs: standard + any new Forever combos use the same IDs
        [1: "WARRIOR", 2: "PALADIN", 3: "HUNTER", 4: "ROGUE", 5: "PRIEST", 6: "DEATHKNIGHT",
         7: "SHAMAN", 8: "MAGE", 9: "WARLOCK", 11: "DRUID"][id]
    }

    static func raceName(for id: Int) -> String? {
        // Race IDs 1-11: original races. 95=Alliance (High Order), 96=Horde (Windshaper) Skyborne (confirmed beta 17 Sep 2026)
        [1: "Human", 2: "Orc", 3: "Dwarf", 4: "Night Elf", 5: "Undead",
         6: "Tauren", 7: "Gnome", 8: "Troll", 10: "Blood Elf", 11: "Draenei",
         95: "High Order Skyborne", 96: "Windshaper Skyborne"][id]
    }

    static func faction(forRaceID id: Int) -> String? {
        if [1, 3, 4, 7, 11, 95].contains(id) { return "Alliance" }   // 95 = Alliance (High Order) Skyborne
        if [2, 5, 6, 8, 10, 96].contains(id) { return "Horde" }      // 96 = Horde (Windshaper) Skyborne
        return nil
    }

    static func characterBlocks(in text: String) -> [(String, String)] {
        let pattern = #"\[\"Default\.[^\"]+\"\]\s*=\s*\{"#
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return [] }
        let ns = text as NSString
        return regex.matches(in: text, range: NSRange(location: 0, length: ns.length)).compactMap { match in
            let header = ns.substring(with: match.range)
            guard let first = header.firstIndex(of: "\""), let last = header.lastIndex(of: "\"") else { return nil }
            let key = String(header[header.index(after: first)..<last])
            var depth = 1, index = match.range.location + match.range.length
            var quoted = false, escaped = false
            while index < ns.length && depth > 0 {
                let char = ns.character(at: index)
                if escaped { escaped = false }
                else if char == 92 { escaped = true }
                else if char == 34 { quoted.toggle() }
                else if !quoted && char == 123 { depth += 1 }
                else if !quoted && char == 125 { depth -= 1 }
                index += 1
            }
            return (key, ns.substring(with: NSRange(location: match.range.location, length: index - match.range.location)))
        }
    }

    static func string(_ key: String, in body: String) -> String? {
        capture(#"\[\""# + NSRegularExpression.escapedPattern(for: key) + #"\"\]\s*=\s*\"([^\"]*)\""#, in: body)
    }

    static func integer(_ key: String, in body: String) -> Int? {
        capture(#"\[\""# + NSRegularExpression.escapedPattern(for: key) + #"\"\]\s*=\s*(\d+)"#, in: body).flatMap(Int.init)
    }

    static func timestamp(_ key: String, in body: String) -> Date? {
        integer(key, in: body).map { Date(timeIntervalSince1970: TimeInterval($0)) }
    }

    static func capture(_ pattern: String, in text: String) -> String? {
        guard let regex = try? NSRegularExpression(pattern: pattern), let match = regex.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)), match.numberOfRanges > 1, let range = Range(match.range(at: 1), in: text) else { return nil }
        return String(text[range])
    }
}

struct DataStoreInventoryAnalyzer: SavedVariablesAnalyzer {
    let identifier = "datastore-inventory"

    private static let bagSizes: [Int: Int] = [
        4496: 6, 5571: 6, 828: 6, 5572: 8, 4240: 8, 5573: 8, 4241: 10,
        5574: 10, 4497: 10, 10050: 12, 10051: 12, 14046: 14, 22246: 16,
        21841: 16, 21843: 18, 21858: 20
    ]
    private static let notableIDs: Set<Int> = [
        2589, 2592, 4306, 4338, 14047, 14256,
        2447, 765, 785, 2450, 2452, 2453, 3355, 3356, 3357, 3358, 3818, 3820, 3821, 8838, 8839,
        2770, 2771, 2772, 2775, 2776, 3858, 7911, 10620, 2835, 2836, 2838,
        2318, 2319, 4234, 4304, 8170, 3371, 3372, 8925
    ]

    func analyze(_ context: AnalyzerContext) throws -> AnalyzerOutput {
        guard let characterFile = context.files.first(where: { $0.lastPathComponent == "DataStore_Characters.lua" }),
              let containerFile = context.files.first(where: { $0.lastPathComponent == "DataStore_Containers.lua" }) else { return AnalyzerOutput() }
        let characterText = try String(contentsOf: characterFile, encoding: .utf8)
        let containerText = try String(contentsOf: containerFile, encoding: .utf8)
        let characters = DataStoreCharacterAnalyzer.arrayRecords(in: characterText, table: "DataStore_Characters_Info")
        let containers = DataStoreCharacterAnalyzer.arrayRecords(in: containerText, table: "DataStore_Containers_Characters")

        let snapshots = characters.enumerated().compactMap { index, body -> CharacterSnapshot? in
            guard index < containers.count, let name = DataStoreCharacterAnalyzer.string("name", in: body) else { return nil }
            let inventory = Self.inventory(from: containers[index])
            return CharacterSnapshot(name: name, realm: nil, faction: nil, race: nil, characterClass: nil, level: nil,
                moneyCopper: nil, zone: nil, subZone: nil, bindLocation: nil, xp: nil, maxXP: nil, restedXP: nil,
                isResting: nil, lastUpdatedAt: nil, professions: [], talentPoints: [:], inventory: inventory, activeQuests: nil,
                knownRecipes: nil)
        }
        return AnalyzerOutput(characters: snapshots)
    }

    static func inventory(from body: String) -> InventorySnapshot {
        let bagLinks = captures(#"\[\"link\"\]\s*=\s*\"[^\"]*Hitem:(\d+)[^\"]*\|h\[([^\]]+)\]"#, in: body)
        let bags = bagLinks.prefix(4).compactMap { values -> BagSnapshot? in
            guard values.count == 2, let id = Int(values[0]) else { return nil }
            return BagSnapshot(itemID: id, name: values[1], slots: bagSizes[id])
        }
        let itemLinks = captures(#"Hitem:(\d+)[^\"]*\|h\[([^\]]+)\]"#, in: body).filter { values in
            guard let first = values.first, let id = Int(first) else { return false }
            return !bagSizes.keys.contains(id)
        }
        let packedItems = packedItemValues(in: body)
        var counts: [Int: (String, Int)] = [:]
        for (offset, values) in itemLinks.enumerated() where values.count == 2 {
            guard let id = Int(values[0]), !bagSizes.keys.contains(id) else { continue }
            let count = offset < packedItems.count ? max(1, packedItems[offset] & 0xFFFF) : 1
            counts[id] = (values[1], (counts[id]?.1 ?? 0) + count)
        }
        let packedBagInfo = DataStoreCharacterAnalyzer.integer("bagInfo", in: body)
        let exactTotal = packedBagInfo.map { $0 & 0x3FF }
        let exactFree = packedBagInfo.map { ($0 >> 10) & 0x3FF }
        let fallbackTotal = 16 + bags.compactMap(\.slots).reduce(0, +)
        let total = exactTotal.flatMap { $0 > 0 ? $0 : nil } ?? fallbackTotal
        let free = min(total, exactFree ?? max(0, total - itemLinks.count))
        let occupied = max(0, total - free)
        let pressure = total == 0 ? "unknown" : free <= 4 ? "high" : free <= 10 ? "medium" : "low"
        let notable = counts.filter { notableIDs.contains($0.key) }.map {
            InventoryItemSnapshot(itemID: $0.key, name: $0.value.0, count: $0.value.1)
        }.sorted { $0.name < $1.name }
        return InventorySnapshot(equippedBags: bags, estimatedTotalSlots: total, occupiedSlots: occupied,
            estimatedFreeSlots: free, bagPressure: pressure, notableItems: notable)
    }

    private static func packedItemValues(in text: String) -> [Int] {
        guard let regex = try? NSRegularExpression(pattern: #"\[\"items\"\]\s*=\s*\{([^}]*)\}"#,
                                                   options: [.dotMatchesLineSeparators]) else { return [] }
        return regex.matches(in: text, range: NSRange(text.startIndex..., in: text)).flatMap { match -> [Int] in
            guard let range = Range(match.range(at: 1), in: text) else { return [] }
            let body = String(text[range])
            guard let numberRegex = try? NSRegularExpression(pattern: #"(?:=\s*)?(\d+)\s*,"#) else { return [] }
            return numberRegex.matches(in: body, range: NSRange(body.startIndex..., in: body)).compactMap {
                guard let valueRange = Range($0.range(at: 1), in: body) else { return nil }
                return Int(body[valueRange])
            }
        }
    }

    private static func captures(_ pattern: String, in text: String) -> [[String]] {
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return [] }
        return regex.matches(in: text, range: NSRange(text.startIndex..., in: text)).map { match in
            (1..<match.numberOfRanges).compactMap { Range(match.range(at: $0), in: text).map { String(text[$0]) } }
        }
    }
}

struct DataStoreQuestAnalyzer: SavedVariablesAnalyzer {
    let identifier = "datastore-quests"

    func analyze(_ context: AnalyzerContext) throws -> AnalyzerOutput {
        guard let characterURL = context.files.first(where: { $0.lastPathComponent == "DataStore_Characters.lua" }),
              let questURL = context.files.first(where: { $0.lastPathComponent == "DataStore_Quests.lua" }) else { return AnalyzerOutput() }
        let characterText = try String(contentsOf: characterURL, encoding: .utf8)
        let questText = try String(contentsOf: questURL, encoding: .utf8)
        let characters = DataStoreCharacterAnalyzer.arrayRecords(in: characterText, table: "DataStore_Characters_Info")
        let questRecords = DataStoreCharacterAnalyzer.arrayRecords(in: questText, table: "DataStore_Quests_Characters")
        let metadata = Self.metadata(in: questText)

        let snapshots = characters.enumerated().compactMap { index, character -> CharacterSnapshot? in
            guard index < questRecords.count, let name = DataStoreCharacterAnalyzer.string("name", in: character) else { return nil }
            let level = (DataStoreCharacterAnalyzer.integer("BaseInfo", in: character) ?? 0) & 0x7F
            let quests = Self.quests(in: questRecords[index], titles: metadata.titles, infos: metadata.infos, characterLevel: level, context: context)
            return CharacterSnapshot(name: name, realm: nil, faction: nil, race: nil, characterClass: nil, level: nil,
                moneyCopper: nil, zone: nil, subZone: nil, bindLocation: nil, xp: nil, maxXP: nil, restedXP: nil,
                isResting: nil, lastUpdatedAt: nil, professions: [], talentPoints: [:], inventory: nil, activeQuests: quests,
                knownRecipes: nil)
        }
        return AnalyzerOutput(characters: snapshots)
    }

    static func quests(in body: String, titles: [Int: String], infos: [Int: Int], characterLevel: Int, context: AnalyzerContext) -> [QuestSnapshot] {
        let headers = stringArray("QuestHeaders", in: body)
        let packed = integerArray("Quests", in: body)
        return packed.map { value in
            let questID = (value >> 6) & 0x3FFFF
            let headerIndex = (value >> 1) & 0x1F
            let level = infos[questID].map { ($0 >> 24) & 0xFF }.flatMap { $0 > 0 ? $0 : nil }
            let difficulty = level.map { questLevel -> String in
                let difference = questLevel - characterLevel
                if difference >= 5 { return "red" }
                if difference >= 3 { return "orange" }
                if difference >= -2 { return "yellow" }
                if difference >= -5 { return "green" }
                return "gray"
            }
            return QuestSnapshot(questID: questID, title: titles[questID], level: level,
                header: headerIndex > 0 && headerIndex <= headers.count ? headers[headerIndex - 1] : nil,
                isComplete: (value & 1) == 1, difficulty: difficulty,
                wowheadURL: Self.wowheadURL(for: questID, context: context))
        }
    }

    static func metadata(in text: String) -> (titles: [Int: String], infos: [Int: Int]) {
        (integerKeyedStrings(in: text, table: "DataStore_Quests_Titles"),
         integerKeyedIntegers(in: text, table: "DataStore_Quests_Infos"))
    }

    static func wowheadURL(for questID: Int, context: AnalyzerContext) -> String {
        // Route Wowhead URL based on installation flavor
        let flavor = context.account.savedVariables.deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().lastPathComponent
        switch flavor {
        case "_forever_":   return "https://www.wowhead.com/forever/quest=\(questID)"
        case "_classic_era_", "_classic_": return "https://www.wowhead.com/classic/quest=\(questID)"
        case "_retail_":    return "https://www.wowhead.com/quest=\(questID)"
        default:            return "https://www.wowhead.com/tbc/quest=\(questID)"  // anniversary + fallback
        }
    }

    private static func stringArray(_ key: String, in body: String) -> [String] {
        guard let table = DataStoreCharacterAnalyzer.nestedTable(key, in: body),
              let regex = try? NSRegularExpression(pattern: #"\"([^\"]*)\"\s*,"#) else { return [] }
        return regex.matches(in: table, range: NSRange(table.startIndex..., in: table)).compactMap {
            Range($0.range(at: 1), in: table).map { String(table[$0]) }
        }
    }

    private static func integerArray(_ key: String, in body: String) -> [Int] {
        guard let table = DataStoreCharacterAnalyzer.nestedTable(key, in: body),
              let regex = try? NSRegularExpression(pattern: #"(?:\[\d+\]\s*=\s*)?(\d+)\s*,"#) else { return [] }
        return regex.matches(in: table, range: NSRange(table.startIndex..., in: table)).compactMap {
            guard let range = Range($0.range(at: 1), in: table) else { return nil }
            return Int(table[range])
        }
    }

    private static func integerKeyedStrings(in text: String, table: String) -> [Int: String] {
        guard let body = topLevelTable(table, in: text),
              let regex = try? NSRegularExpression(pattern: #"\[(\d+)\]\s*=\s*\"([^\"]*)\""#) else { return [:] }
        return Dictionary(uniqueKeysWithValues: regex.matches(in: body, range: NSRange(body.startIndex..., in: body)).compactMap {
            guard let keyRange = Range($0.range(at: 1), in: body), let valueRange = Range($0.range(at: 2), in: body), let key = Int(body[keyRange]) else { return nil }
            return (key, String(body[valueRange]))
        })
    }

    private static func integerKeyedIntegers(in text: String, table: String) -> [Int: Int] {
        guard let body = topLevelTable(table, in: text),
              let regex = try? NSRegularExpression(pattern: #"\[(\d+)\]\s*=\s*(\d+)"#) else { return [:] }
        return Dictionary(uniqueKeysWithValues: regex.matches(in: body, range: NSRange(body.startIndex..., in: body)).compactMap {
            guard let keyRange = Range($0.range(at: 1), in: body), let valueRange = Range($0.range(at: 2), in: body),
                  let key = Int(body[keyRange]), let value = Int(body[valueRange]) else { return nil }
            return (key, value)
        })
    }

    private static func topLevelTable(_ name: String, in text: String) -> String? {
        guard let range = text.range(of: name), let equals = text[range.upperBound...].firstIndex(of: "="),
              let open = text[equals...].firstIndex(of: "{") else { return nil }
        let ns = text as NSString
        let start = NSRange(text.startIndex..<open, in: text).length
        var depth = 0, quoted = false, escaped = false, index = start
        while index < ns.length {
            let char = ns.character(at: index)
            if escaped { escaped = false }
            else if char == 92 && quoted { escaped = true }
            else if char == 34 { quoted.toggle() }
            else if !quoted && char == 123 { depth += 1 }
            else if !quoted && char == 125 { depth -= 1; if depth == 0 { return ns.substring(with: NSRange(location: start, length: index - start + 1)) } }
            index += 1
        }
        return nil
    }
}

struct DataStoreCraftRecipeAnalyzer: SavedVariablesAnalyzer {
    let identifier = "datastore-craft-recipes"

    private static let knownNames: [Int: String] = [
        2329: "Elixir of Lion's Strength", 7183: "Elixir of Minor Defense",
        3176: "Strong Troll's Blood Potion", 3173: "Lesser Mana Potion",
        3447: "Healing Potion", 2337: "Lesser Healing Potion",
        4508: "Discolored Healing Potion", 2330: "Minor Healing Potion",
        2387: "Linen Cloak", 2393: "White Linen Shirt", 2397: "Reinforced Linen Cape",
        2963: "Bolt of Linen Cloth", 2964: "Bolt of Woolen Cloth", 3758: "Green Woolen Bag",
        3839: "Bolt of Silk Cloth", 3848: "Double-stitched Woolen Shoulders",
        3871: "Formal White Shirt", 3915: "Brown Linen Shirt", 6688: "Red Woolen Bag",
        8760: "Azure Silk Hood", 8762: "Silk Headband", 8776: "Linen Belt",
        12044: "Simple Linen Pants", 12046: "Simple Kilt",
        3275: "Linen Bandage", 3276: "Heavy Linen Bandage", 3277: "Wool Bandage",
        3278: "Heavy Wool Bandage", 7934: "Anti-Venom",
        7418: "Enchant Bracer - Minor Health", 7421: "Runed Copper Rod",
        7428: "Enchant Bracer - Minor Deflection", 14293: "Lesser Magic Wand",
        2149: "Handstitched Leather Boots", 2152: "Light Armor Kit", 2881: "Light Leather",
        3756: "Embossed Leather Gloves", 7126: "Handstitched Leather Vest",
        9058: "Handstitched Leather Cloak", 9059: "Handstitched Leather Bracers",
        25255: "Delicate Copper Wire", 25493: "Braided Copper Ring",
        26925: "Woven Copper Ring", 32259: "Rough Stone Statue",
        2657: "Smelt Copper", 2660: "Rough Sharpening Stone", 2662: "Copper Chain Pants",
        2663: "Copper Bracers", 3115: "Rough Weightstone", 12260: "Rough Copper Vest"
    ]

    private static let canonicalProfessions: [Int: String] = [
        2329: "Alchemy", 7183: "Alchemy", 3176: "Alchemy", 3173: "Alchemy", 3447: "Alchemy",
        2337: "Alchemy", 4508: "Alchemy", 2330: "Alchemy",
        2387: "Tailoring", 2393: "Tailoring", 2397: "Tailoring", 2963: "Tailoring",
        2964: "Tailoring", 3758: "Tailoring", 3839: "Tailoring", 3848: "Tailoring",
        3871: "Tailoring", 3915: "Tailoring", 6688: "Tailoring", 8760: "Tailoring",
        8762: "Tailoring", 8776: "Tailoring", 12044: "Tailoring", 12046: "Tailoring",
        3275: "First Aid", 3276: "First Aid", 3277: "First Aid", 3278: "First Aid", 7934: "First Aid",
        7418: "Enchanting", 7421: "Enchanting", 7428: "Enchanting", 14293: "Enchanting",
        2149: "Leatherworking", 2152: "Leatherworking", 2881: "Leatherworking",
        3756: "Leatherworking", 7126: "Leatherworking", 9058: "Leatherworking", 9059: "Leatherworking",
        25255: "Jewelcrafting", 25493: "Jewelcrafting", 26925: "Jewelcrafting", 32259: "Jewelcrafting",
        2657: "Mining", 2660: "Blacksmithing", 2662: "Blacksmithing", 2663: "Blacksmithing",
        3115: "Blacksmithing", 12260: "Blacksmithing"
    ]

    func analyze(_ context: AnalyzerContext) throws -> AnalyzerOutput {
        guard let characterURL = context.files.first(where: { $0.lastPathComponent == "DataStore_Characters.lua" }),
              let craftURL = context.files.first(where: { $0.lastPathComponent == "DataStore_Crafts.lua" }) else { return AnalyzerOutput() }
        let characterText = try String(contentsOf: characterURL, encoding: .utf8)
        let craftText = try String(contentsOf: craftURL, encoding: .utf8)
        let characters = DataStoreCharacterAnalyzer.arrayRecords(in: characterText, table: "DataStore_Characters_Info")
        let crafts = DataStoreCharacterAnalyzer.arrayRecords(in: craftText, table: "DataStore_Crafts_Characters")
        let snapshots = characters.enumerated().compactMap { index, character -> CharacterSnapshot? in
            guard index < crafts.count, let name = DataStoreCharacterAnalyzer.string("name", in: character) else { return nil }
            let recipes = Self.recipes(in: crafts[index])
            return CharacterSnapshot(name: name, realm: nil, faction: nil, race: nil, characterClass: nil, level: nil,
                moneyCopper: nil, zone: nil, subZone: nil, bindLocation: nil, xp: nil, maxXP: nil, restedXP: nil,
                isResting: nil, lastUpdatedAt: nil, professions: [], talentPoints: [:], inventory: nil, activeQuests: nil,
                knownRecipes: recipes)
        }
        return AnalyzerOutput(characters: snapshots)
    }

    static func recipes(in craftRecord: String) -> [RecipeSnapshot] {
        guard let professions = DataStoreCharacterAnalyzer.nestedTable("Professions", in: craftRecord) else { return [] }
        let records = DataStoreCharacterAnalyzer.arrayRecords(in: "RecipeProfessions = \(professions)", table: "RecipeProfessions")
        let parsed = records.flatMap { record -> [RecipeSnapshot] in
            guard let profession = DataStoreCharacterAnalyzer.string("Name", in: record),
                  let crafts = DataStoreCharacterAnalyzer.nestedTable("Crafts", in: record),
                  let regex = try? NSRegularExpression(pattern: #"\"([1-4])\|(\d+)\""#) else { return [] }
            return regex.matches(in: crafts, range: NSRange(crafts.startIndex..., in: crafts)).compactMap { match in
                guard let codeRange = Range(match.range(at: 1), in: crafts),
                      let idRange = Range(match.range(at: 2), in: crafts),
                      let spellID = Int(crafts[idRange]) else { return nil }
                let difficulty = ["1": "orange", "2": "yellow", "3": "green", "4": "gray"][String(crafts[codeRange])] ?? "unknown"
                return RecipeSnapshot(profession: profession, spellID: spellID, name: Self.knownNames[spellID], difficulty: difficulty)
            }
        }
        return Dictionary(grouping: parsed, by: \RecipeSnapshot.spellID).compactMap { spellID, copies in
            if let canonical = canonicalProfessions[spellID] {
                let source = copies.first(where: { $0.profession == canonical }) ?? copies[0]
                return RecipeSnapshot(profession: canonical, spellID: spellID,
                    name: knownNames[spellID] ?? source.name, difficulty: source.difficulty)
            }
            if copies.count > 1, let nonEnchanting = copies.first(where: { $0.profession != "Enchanting" }) {
                return nonEnchanting
            }
            return copies.first
        }.sorted { ($0.profession, $0.spellID) < ($1.profession, $1.spellID) }
    }
}

struct AuctionatorScanAnalyzer: SavedVariablesAnalyzer {
    let identifier = "auctionator-scan"

    func analyze(_ context: AnalyzerContext) throws -> AnalyzerOutput {
        guard let file = context.files.first(where: { $0.lastPathComponent == "Auctionator.lua" }) else { return AnalyzerOutput() }
        // Auctionator stores its compressed price database in the same Lua file, so it may
        // contain bytes that are not valid UTF-8. Lossy decoding preserves the metadata keys.
        let text = String(decoding: try Data(contentsOf: file), as: UTF8.self)
        let timestamp = DataStoreCharacterAnalyzer.integer("TimeOfLastGetAllScan", in: text)
        let marketRegex = try? NSRegularExpression(pattern: #"AUCTIONATOR_PRICE_DATABASE\s*=\s*\{\s*\[\"([^\"]+)\"\]"#,
                                                   options: [.dotMatchesLineSeparators])
        let market = marketRegex?.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)).flatMap { match in
            Range(match.range(at: 1), in: text).map { String(text[$0]) }
        }
        var values: [String: String] = [:]
        if let timestamp {
            values["lastFullScanAt"] = ISO8601DateFormatter().string(from: Date(timeIntervalSince1970: TimeInterval(timestamp)))
            values["scanAgeHours"] = String(format: "%.1f", max(0, Date().timeIntervalSince1970 - TimeInterval(timestamp)) / 3600)
        }
        if let market { values["market"] = market }
        return values.isEmpty ? AnalyzerOutput() : AnalyzerOutput(findings: [.init(analyzer: identifier, values: values)])
    }
}
