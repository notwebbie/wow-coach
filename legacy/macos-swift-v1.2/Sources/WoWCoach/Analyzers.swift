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

    static let standard = AnalyzerRegistry(analyzers: [DataStoreCharacterAnalyzer(), DataStoreInventoryAnalyzer(), AddonMetadataAnalyzer()])

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
        let text = try String(contentsOf: file, encoding: .utf8)
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
                inventory: nil
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
                inventory: nil
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
        let start = text.distance(from: text.startIndex, to: open)
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
        [1: "WARRIOR", 2: "PALADIN", 3: "HUNTER", 4: "ROGUE", 5: "PRIEST", 6: "DEATHKNIGHT", 7: "SHAMAN", 8: "MAGE", 9: "WARLOCK", 11: "DRUID"][id]
    }

    static func raceName(for id: Int) -> String? {
        [1: "Human", 2: "Orc", 3: "Dwarf", 4: "Night Elf", 5: "Undead", 6: "Tauren", 7: "Gnome", 8: "Troll", 10: "Blood Elf", 11: "Draenei"][id]
    }

    static func faction(forRaceID id: Int) -> String? {
        if [1, 3, 4, 7, 11].contains(id) { return "Alliance" }
        if [2, 5, 6, 8, 10].contains(id) { return "Horde" }
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
    private static let notableIDs: Set<Int> = [2589, 2592, 4306, 4338, 2447, 765, 785, 2450, 2452, 2453, 2770, 2771, 2772, 2835, 2318, 2319, 4234]

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
                isResting: nil, lastUpdatedAt: nil, professions: [], talentPoints: [:], inventory: inventory)
        }
        return AnalyzerOutput(characters: snapshots)
    }

    static func inventory(from body: String) -> InventorySnapshot {
        let bagLinks = captures(#"\[\"link\"\]\s*=\s*\"[^\"]*Hitem:(\d+)[^\"]*\|h\[([^\]]+)\]"#, in: body)
        let bags = bagLinks.prefix(4).compactMap { values -> BagSnapshot? in
            guard values.count == 2, let id = Int(values[0]) else { return nil }
            return BagSnapshot(itemID: id, name: values[1], slots: bagSizes[id])
        }
        let itemLinks = captures(#"Hitem:(\d+)[^\"]*\|h\[([^\]]+)\]"#, in: body)
        var counts: [Int: (String, Int)] = [:]
        for values in itemLinks where values.count == 2 {
            guard let id = Int(values[0]), !bagSizes.keys.contains(id) else { continue }
            counts[id] = (values[1], (counts[id]?.1 ?? 0) + 1)
        }
        let occupied = counts.values.reduce(0) { $0 + $1.1 }
        let total = 16 + bags.compactMap(\.slots).reduce(0, +)
        let free = max(0, total - occupied)
        let pressure = total == 0 ? "unknown" : free <= 4 ? "high" : free <= 10 ? "medium" : "low"
        let notable = counts.filter { notableIDs.contains($0.key) }.map {
            InventoryItemSnapshot(itemID: $0.key, name: $0.value.0, count: $0.value.1)
        }.sorted { $0.name < $1.name }
        return InventorySnapshot(equippedBags: bags, estimatedTotalSlots: total, occupiedSlots: occupied,
            estimatedFreeSlots: free, bagPressure: pressure, notableItems: notable)
    }

    private static func captures(_ pattern: String, in text: String) -> [[String]] {
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return [] }
        return regex.matches(in: text, range: NSRange(text.startIndex..., in: text)).map { match in
            (1..<match.numberOfRanges).compactMap { Range(match.range(at: $0), in: text).map { String(text[$0]) } }
        }
    }
}
