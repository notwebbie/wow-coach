import Foundation

actor ReportBuilder {
    private let registry: AnalyzerRegistry
    private let allowedPrefixes = ["Questie", "GatherMate2", "Auctionator", "Altoholic", "DataStore", "Pawn", "SavedInstances", "Bagnon", "Leatrix"]

    init(registry: AnalyzerRegistry = .standard) { self.registry = registry }

    func create(installation: WoWInstallation, accounts: [AccountSource], destinationDirectory: URL, coachingProfile: CoachingProfile?) throws -> ReportResult {
        let fm = FileManager.default
        let stamp = ISO8601DateFormatter().string(from: Date()).replacingOccurrences(of: ":", with: "-")
        let work = fm.temporaryDirectory.appendingPathComponent("WoWCoach-\(UUID().uuidString)", isDirectory: true)
        let bundle = work.appendingPathComponent("WoWCoachReport-\(stamp)", isDirectory: true)
        defer { try? fm.removeItem(at: work) }
        try fm.createDirectory(at: bundle, withIntermediateDirectories: true)
        var summaries: [AccountSummary] = [], warnings: [String] = []
        var copied = 0

        for account in accounts {
            let files = try eligibleFiles(in: account.savedVariables)
            let context = AnalyzerContext(account: account, files: files)
            let output = registry.run(context)
            warnings += output.warnings.map { "\(account.name): \($0)" }
            let raw = bundle.appendingPathComponent("raw").appendingPathComponent(safeName(account.name), isDirectory: true)
            try fm.createDirectory(at: raw, withIntermediateDirectories: true)
            for file in files {
                try fm.copyItem(at: file, to: raw.appendingPathComponent(file.lastPathComponent))
                copied += 1
            }
            let grouped = Dictionary(grouping: files) { $0.deletingPathExtension().lastPathComponent }
            let addons = grouped.map { name, urls in
                AddonSnapshot(name: name, files: urls.map(\.lastPathComponent).sorted(), byteCount: urls.reduce(0) { total, url in
                    total + ((try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
                })
            }.sorted { $0.name < $1.name }
            summaries.append(.init(account: account.name, sourcePath: account.savedVariables.path, addons: addons, characters: output.characters, findings: output.findings))
        }

        let previous = try? loadPreviousSummary(flavor: installation.flavor, accounts: accounts)
        let deltas = Self.progressDeltas(previous: previous, current: summaries)
        let summary = ReportSummary(schemaVersion: 3, generatedAt: Date(), appVersion: "1.2.0", installationFlavor: installation.flavor,
            accounts: summaries, warnings: warnings, coachingProfile: coachingProfile, progressSincePreviousReport: deltas)
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]; encoder.dateEncodingStrategy = .iso8601
        let summaryURL = bundle.appendingPathComponent("summary.json")
        try encoder.encode(summary).write(to: summaryURL, options: .atomic)
        try Self.writeHTML(summary, to: bundle.appendingPathComponent("report.html"))
        try Self.writeReadme(to: bundle.appendingPathComponent("README.txt"))
        try fm.createDirectory(at: destinationDirectory, withIntermediateDirectories: true)
        let zip = destinationDirectory.appendingPathComponent("WoWCoachReport-\(stamp).zip")
        try zipDirectory(bundle, to: zip)
        try saveLatest(summary, flavor: installation.flavor, accounts: accounts)
        return ReportResult(zipURL: zip, accountCount: accounts.count, fileCount: copied)
    }

    private func eligibleFiles(in directory: URL) throws -> [URL] {
        let urls = try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey], options: [.skipsHiddenFiles])
        return urls.filter { url in
            guard url.pathExtension.lowercased() == "lua", allowedPrefixes.contains(where: { prefix in
                url.lastPathComponent.range(of: prefix, options: [.anchored, .caseInsensitive]) != nil
            }) else { return false }
            let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
            return values?.isRegularFile == true && values?.isSymbolicLink != true
        }.sorted { $0.lastPathComponent < $1.lastPathComponent }
    }

    private func historyDirectory(flavor: String, accounts: [AccountSource]) throws -> URL {
        let root = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            .appendingPathComponent("WoWCoach/History", isDirectory: true)
        let directory = root.appendingPathComponent(safeName("\(flavor)-\(accounts.map(\.name).sorted().joined(separator: "-"))"), isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }

    private func loadPreviousSummary(flavor: String, accounts: [AccountSource]) throws -> ReportSummary {
        let decoder = JSONDecoder(); decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(ReportSummary.self, from: Data(contentsOf: try historyDirectory(flavor: flavor, accounts: accounts).appendingPathComponent("latest.json")))
    }

    private func saveLatest(_ summary: ReportSummary, flavor: String, accounts: [AccountSource]) throws {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]; encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(summary)
        let directory = try historyDirectory(flavor: flavor, accounts: accounts)
        try data.write(to: directory.appendingPathComponent("latest.json"), options: .atomic)
        let stamp = ISO8601DateFormatter().string(from: summary.generatedAt).replacingOccurrences(of: ":", with: "-")
        try data.write(to: directory.appendingPathComponent("report-\(stamp).json"), options: .atomic)
    }

    static func progressDeltas(previous: ReportSummary?, current: [AccountSummary]) -> [ProgressDelta] {
        let old = Dictionary(uniqueKeysWithValues: (previous?.accounts.flatMap(\.characters) ?? []).map { ($0.name, $0) })
        return current.flatMap(\.characters).compactMap { character -> ProgressDelta? in
            guard let before = old[character.name] else { return nil }
            let professionBefore = Dictionary(uniqueKeysWithValues: before.professions.map { ($0.name, $0.rank) })
            let changedPairs: [(String, Int)] = character.professions.compactMap { profession -> (String, Int)? in
                guard let oldRank = professionBefore[profession.name], oldRank != profession.rank else { return nil }
                return (profession.name, profession.rank - oldRank)
            }
            let professionChanges = Dictionary(uniqueKeysWithValues: changedPairs)
            let levelChanged = before.level != character.level
            let xpGain: Int? = levelChanged ? nil : {
                guard let oldXP = before.xp, let newXP = character.xp else { return nil }
                return newXP - oldXP
            }()
            let moneyChange: Int? = {
                guard let oldMoney = before.moneyCopper, let newMoney = character.moneyCopper else { return nil }
                return newMoney - oldMoney
            }()
            guard levelChanged || xpGain != 0 || moneyChange != 0 || !professionChanges.isEmpty else { return nil }
            return ProgressDelta(character: character.name, levelBefore: before.level, levelAfter: character.level,
                xpGained: xpGain, moneyChangeCopper: moneyChange, professionChanges: professionChanges)
        }.sorted { $0.character < $1.character }
    }

    private func zipDirectory(_ directory: URL, to zip: URL) throws {
        let process = Process(); process.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        process.arguments = ["-c", "-k", "--sequesterRsrc", "--keepParent", directory.path, zip.path]
        let errorPipe = Pipe(); process.standardError = errorPipe
        try process.run(); process.waitUntilExit()
        if process.terminationStatus != 0 {
            let message = String(data: errorPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "ZIP creation failed"
            throw NSError(domain: "WoWCoach", code: Int(process.terminationStatus), userInfo: [NSLocalizedDescriptionKey: message])
        }
    }

    private func safeName(_ value: String) -> String {
        value.replacingOccurrences(of: #"[^A-Za-z0-9._-]"#, with: "_", options: .regularExpression)
    }

    private static func writeReadme(to url: URL) throws {
        try "WoW Coach report v1.2\n\nreport.html is a readable overview. summary.json contains structured character, progress, strategy, profession, and inventory data. raw/ contains supported live addon SavedVariables; backup files are excluded. Review before sharing because raw files may contain character names, inventory, auction history, mail, and addon settings.\n".write(to: url, atomically: true, encoding: .utf8)
    }

    private static func writeHTML(_ summary: ReportSummary, to url: URL) throws {
        let rows = summary.accounts.flatMap(\.characters).map { character in
            let money = character.moneyCopper.map { String(format: "%dg %ds %dc", $0 / 10000, ($0 / 100) % 100, $0 % 100) } ?? "—"
            let bags = character.inventory.map { "\($0.estimatedFreeSlots) free / \($0.estimatedTotalSlots)" } ?? "—"
            let professions = character.professions.map { "\($0.name) \($0.rank)" }.joined(separator: ", ")
            return "<tr><td>\(escape(character.name))</td><td>\(escape(character.faction ?? "—"))</td><td>\(character.level.map(String.init) ?? "—")</td><td>\(escape(character.zone ?? "—"))</td><td>\(money)</td><td>\(bags)</td><td>\(escape(professions))</td></tr>"
        }.joined(separator: "\n")
        let objective = escape(summary.coachingProfile?.objective ?? "No progression objective configured")
        let html = """
        <!doctype html><html><head><meta charset="utf-8"><title>WoW Coach Report</title>
        <style>body{font:15px -apple-system;margin:40px;background:#11151b;color:#e8edf4}h1{margin-bottom:4px}.goal{color:#c084fc}table{border-collapse:collapse;width:100%;margin-top:24px}th,td{padding:10px;border-bottom:1px solid #303743;text-align:left}th{color:#9ca3af}</style></head>
        <body><h1>WoW Coach</h1><div class="goal">\(objective)</div><table><thead><tr><th>Character</th><th>Faction</th><th>Level</th><th>Zone</th><th>Gold</th><th>Bags</th><th>Professions</th></tr></thead><tbody>\(rows)</tbody></table></body></html>
        """
        try html.write(to: url, atomically: true, encoding: .utf8)
    }

    private static func escape(_ value: String) -> String {
        value.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "<", with: "&lt;").replacingOccurrences(of: ">", with: "&gt;")
    }
}
