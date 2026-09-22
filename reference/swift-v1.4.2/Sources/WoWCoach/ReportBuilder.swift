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

        let previous = loadPreviousSummary(flavor: installation.flavor, accounts: accounts, destinationDirectory: destinationDirectory)
        let deltas = Self.progressDeltas(previous: previous, current: summaries)
        let dashboard = Self.dashboard(accounts: summaries, profile: coachingProfile, progress: deltas, now: Date())
        let summary = ReportSummary(schemaVersion: 5, generatedAt: Date(), appVersion: "1.4.2", installationFlavor: installation.flavor,
            accounts: summaries, warnings: warnings, coachingProfile: coachingProfile, progressSincePreviousReport: deltas,
            previousReportGeneratedAt: previous?.generatedAt, coachingDashboard: dashboard)
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]; encoder.dateEncodingStrategy = .iso8601
        let summaryURL = bundle.appendingPathComponent("summary.json")
        try encoder.encode(summary).write(to: summaryURL, options: .atomic)
        if let previous { try encoder.encode(previous).write(to: bundle.appendingPathComponent("previous-summary.json"), options: .atomic) }
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

    private func loadPreviousSummary(flavor: String, accounts: [AccountSource], destinationDirectory: URL) -> ReportSummary? {
        let decoder = JSONDecoder(); decoder.dateDecodingStrategy = .iso8601
        if let directory = try? historyDirectory(flavor: flavor, accounts: accounts),
           let data = try? Data(contentsOf: directory.appendingPathComponent("latest.json")),
           let summary = try? decoder.decode(ReportSummary.self, from: data) { return summary }
        return Self.mostRecentSummary(in: destinationDirectory, decoder: decoder)
    }

    private static func mostRecentSummary(in directory: URL, decoder: JSONDecoder) -> ReportSummary? {
        let files = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: [.contentModificationDateKey], options: [.skipsHiddenFiles])) ?? []
        let candidates = files.filter { $0.pathExtension.lowercased() == "zip" && $0.lastPathComponent.hasPrefix("WoWCoachReport-") }
            .sorted { ((try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast) > ((try? $1.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast) }
        for zip in candidates {
            let process = Process(); process.executableURL = URL(fileURLWithPath: "/usr/bin/unzip")
            process.arguments = ["-p", zip.path, "*/summary.json"]
            let output = Pipe(); process.standardOutput = output; process.standardError = FileHandle.nullDevice
            do { try process.run(); process.waitUntilExit() } catch { continue }
            guard process.terminationStatus == 0 else { continue }
            let data = output.fileHandleForReading.readDataToEndOfFile()
            if let summary = try? decoder.decode(ReportSummary.self, from: data) { return summary }
        }
        return nil
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
            let xpGain: Int? = {
                guard let oldXP = before.xp, let newXP = character.xp else { return nil }
                if let oldLevel = before.level, let newLevel = character.level, newLevel == oldLevel + 1,
                   let oldMaximum = before.maxXP { return max(0, oldMaximum - oldXP) + newXP }
                guard !levelChanged else { return nil }
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

    static func dashboard(accounts: [AccountSummary], profile: CoachingProfile?, progress: [ProgressDelta], now: Date) -> CoachingDashboard {
        let characters = accounts.flatMap(\.characters)
        let bankOnly = Set((profile?.bankOnlyCharacters ?? []).map { $0.lowercased() })
        let eligible = characters.filter { !bankOnly.contains($0.name.lowercased()) }
        let configuredRotation = (profile?.rotationCharacters ?? []).compactMap { wanted in
            eligible.first { $0.name.caseInsensitiveCompare(wanted) == .orderedSame }
        }
        let rotation = configuredRotation.isEmpty ? eligible.sorted { $0.name < $1.name } : configuredRotation
        let progressedNames = Set(progress.compactMap { delta -> String? in
            if (delta.xpGained ?? 0) > 0 || delta.levelBefore != delta.levelAfter { return delta.character.lowercased() }
            return nil
        })
        let mostRecentlyPlayed = rotation.filter { progressedNames.contains($0.name.lowercased()) }
            .max { ($0.lastUpdatedAt ?? .distantPast) < ($1.lastUpdatedAt ?? .distantPast) }
            ?? rotation.max { ($0.lastUpdatedAt ?? .distantPast) < ($1.lastUpdatedAt ?? .distantPast) }
        let recommended: CharacterSnapshot? = {
            guard !rotation.isEmpty else { return nil }
            guard let mostRecentlyPlayed, let index = rotation.firstIndex(where: { $0.name == mostRecentlyPlayed.name }) else { return rotation.first }
            return rotation[(index + 1) % rotation.count]
        }()
        var warnings: [String] = []
        let roster = rotation.enumerated().map { offset, character -> CoachingRosterEntry in
            let estimate = Self.estimatedRestedXP(for: character, now: now)
            let expectedTalentPoints = max(0, (character.level ?? 0) - 9)
            let spentTalentPoints = character.talentPoints.values.reduce(0, +)
            let unspent = max(0, expectedTalentPoints - spentTalentPoints)
            var reminders: [String] = []
            if let level = character.level, level >= 10, level.isMultiple(of: 2) { reminders.append("Check the class trainer for level \(level) abilities.") }
            if unspent > 0 { reminders.append("Spend \(unspent) unspent talent point\(unspent == 1 ? "" : "s").") }
            if (character.activeQuests?.count ?? 0) >= 23 { reminders.append("Quest log is nearly full.") }
            for profession in character.professions where profession.maximumRank - profession.rank <= 5 {
                reminders.append("\(profession.name) is close to its \(profession.maximumRank) cap.")
            }
            return CoachingRosterEntry(character: character.name, rotationPosition: offset + 1,
                lastUpdatedAt: character.lastUpdatedAt, observedRestedXP: character.restedXP,
                estimatedRestedXP: estimate.value, restedXPIsEstimated: estimate.isEstimated,
                questCount: character.activeQuests?.count ?? 0, freeBagSlots: character.inventory?.estimatedFreeSlots,
                unspentTalentPoints: unspent, reminders: reminders)
        }
        for character in characters {
            if character.inventory?.bagPressure == "high" { warnings.append("\(character.name) has high bag pressure.") }
            if let updated = character.lastUpdatedAt, now.timeIntervalSince(updated) > 72 * 3600 {
                warnings.append("\(character.name)'s data is more than three days old.")
            }
            let hard = character.activeQuests?.filter { $0.difficulty == "orange" || $0.difficulty == "red" }.count ?? 0
            if hard > 0 { warnings.append("\(character.name) has \(hard) orange/red active quest\(hard == 1 ? "" : "s").") }
            if (character.activeQuests?.count ?? 0) >= 23 { warnings.append("\(character.name)'s quest log is nearly full.") }
            let expected = max(0, (character.level ?? 0) - 9)
            let spent = character.talentPoints.values.reduce(0, +)
            if spent < expected { warnings.append("\(character.name) may have \(expected - spent) unspent talent point\(expected - spent == 1 ? "" : "s").") }
        }
        for account in accounts {
            if let scan = account.findings.first(where: { $0.analyzer == "auctionator-scan" }),
               let age = scan.values["scanAgeHours"].flatMap(Double.init), age > 24 {
                warnings.append("The \(scan.values["market"] ?? "Auction House") full scan is \(Int(age)) hours old.")
            }
        }
        let reason = recommended.map { character -> String in
            let previousName = mostRecentlyPlayed?.name ?? "the previous character"
            let estimate = Self.estimatedRestedXP(for: character, now: now)
            let rested = estimate.value.map { " About \($0) rested XP is \(estimate.isEstimated ? "estimated" : "recorded")." } ?? ""
            return "Next in the configured rotation after \(previousName).\(rested)"
        }
        return CoachingDashboard(recommendedCharacter: recommended?.name, reason: reason, warnings: Array(Set(warnings)).sorted(), roster: roster)
    }

    static func estimatedRestedXP(for character: CharacterSnapshot, now: Date) -> (value: Int?, isEstimated: Bool) {
        guard let observed = character.restedXP else { return (nil, false) }
        guard character.isResting == true, let updated = character.lastUpdatedAt, let maxXP = character.maxXP else {
            return (observed, false)
        }
        let elapsed = max(0, now.timeIntervalSince(updated))
        guard elapsed >= 3600 else { return (observed, false) }
        let earned = Int((Double(maxXP) * elapsed) / (160 * 3600))
        return (min(Int(Double(maxXP) * 1.5), observed + earned), true)
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
        try "WoW Coach report v1.4.1\n\nreport.html is a readable overview. summary.json contains structured character, rotation, estimated rested XP, progress, quests, recipes, Auction House scan metadata, professions, and inventory data. previous-summary.json preserves the comparison baseline when available. raw/ contains supported live addon SavedVariables; backup files are excluded. Review before sharing because raw files may contain character names, inventory, auction history, mail, and addon settings.\n".write(to: url, atomically: true, encoding: .utf8)
    }

    private static func writeHTML(_ summary: ReportSummary, to url: URL) throws {
        let rows = summary.accounts.flatMap(\.characters).map { character in
            let money = character.moneyCopper.map { String(format: "%dg %ds %dc", $0 / 10000, ($0 / 100) % 100, $0 % 100) } ?? "—"
            let bags = character.inventory.map { "\($0.estimatedFreeSlots) free / \($0.estimatedTotalSlots)" } ?? "—"
            let professions = character.professions.map { "\($0.name) \($0.rank)" }.joined(separator: ", ")
            let quests = character.activeQuests.map { "\($0.count) active" } ?? "—"
            return "<tr><td>\(escape(character.name))</td><td>\(escape(character.faction ?? "—"))</td><td>\(character.level.map(String.init) ?? "—")</td><td>\(escape(character.zone ?? "—"))</td><td>\(money)</td><td>\(bags)</td><td>\(escape(professions))</td><td>\(quests)</td></tr>"
        }.joined(separator: "\n")
        let objective = escape(summary.coachingProfile?.objective ?? "No progression objective configured")
        let recommendation = summary.coachingDashboard?.recommendedCharacter.map { "<div class=\"recommendation\">Play next: <strong>\(escape($0))</strong> — \(escape(summary.coachingDashboard?.reason ?? ""))</div>" } ?? ""
        let progress = summary.progressSincePreviousReport.map { delta in
            let xp = delta.xpGained.map { ", +\($0) XP" } ?? ""
            return "<li>\(escape(delta.character)): level \(delta.levelBefore.map(String.init) ?? "—") → \(delta.levelAfter.map(String.init) ?? "—")\(xp)</li>"
        }.joined()
        let warningList = (summary.coachingDashboard?.warnings ?? []).map { "<li>\(escape($0))</li>" }.joined()
        let html = """
        <!doctype html><html><head><meta charset="utf-8"><title>WoW Coach Report</title>
        <style>body{font:15px -apple-system;margin:40px;background:#11151b;color:#e8edf4}h1{margin-bottom:4px}.goal{color:#c084fc}.recommendation{background:#202735;border:1px solid #394357;border-radius:12px;padding:16px;margin:22px 0}table{border-collapse:collapse;width:100%;margin-top:24px}th,td{padding:10px;border-bottom:1px solid #303743;text-align:left}th{color:#9ca3af}.warnings{color:#fbbf24}</style></head>
        <body><h1>WoW Coach</h1><div class="goal">\(objective)</div>\(recommendation)<h2>Progress</h2><ul>\(progress.isEmpty ? "<li>This report establishes a new comparison baseline.</li>" : progress)</ul><table><thead><tr><th>Character</th><th>Faction</th><th>Level</th><th>Zone</th><th>Gold</th><th>Bags</th><th>Professions</th><th>Quests</th></tr></thead><tbody>\(rows)</tbody></table><div class="warnings"><h2>Attention</h2><ul>\(warningList)</ul></div></body></html>
        """
        try html.write(to: url, atomically: true, encoding: .utf8)
    }

    private static func escape(_ value: String) -> String {
        value.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "<", with: "&lt;").replacingOccurrences(of: ">", with: "&gt;")
    }
}
