import AppKit
import Foundation
import SwiftUI

@MainActor
final class AppModel: ObservableObject {
    @Published var installations: [WoWInstallation] = []
    @Published var selectedInstallation: WoWInstallation?
    @Published var accounts: [AccountSource] = []
    @Published var selectedAccountIDs: Set<String> = []
    @Published var customRoot: URL?
    @Published var destination = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Desktop")
    @Published var isWorking = false
    @Published var status = "Ready to discover your WoW installation."
    @Published var lastReport: URL?
    @Published var showingSettings = false
    @Published var primaryCharacter: String
    @Published var secondaryCharacter: String
    @Published var bankOnlyCharacters: String
    @Published var progressionObjective: String
    @Published var targetDate: String
    @Published var rotationCharacters: String

    private let discovery = WoWDiscovery()
    private let builder = ReportBuilder()

    init() {
        let defaults = UserDefaults.standard
        primaryCharacter = defaults.string(forKey: "primaryCharacter") ?? "Examplemage"
        secondaryCharacter = defaults.string(forKey: "secondaryCharacter") ?? "Examplelock"
        bankOnlyCharacters = defaults.string(forKey: "bankOnlyCharacters") ?? "Examplebank"
        progressionObjective = defaults.string(forKey: "progressionObjective") ?? "Reach level 70 and experience TBC endgame"
        targetDate = defaults.string(forKey: "targetDate") ?? "December 2026"
        rotationCharacters = defaults.string(forKey: "rotationCharacters") ?? "Examplemage, Examplelock, Examplepriest, Exampledruid"
        refresh()
    }

    func refresh() {
        installations = discovery.installations(customRoot: customRoot)
        if selectedInstallation == nil || !installations.contains(selectedInstallation!) { selectedInstallation = installations.first }
        reloadAccounts()
        status = installations.isEmpty ? "No Classic installation found. Choose it in Settings." : "Found \(installations.count) WoW installation\(installations.count == 1 ? "" : "s")."
    }

    func reloadAccounts() {
        accounts = selectedInstallation.map(discovery.accounts) ?? []
        selectedAccountIDs = Set(accounts.map(\.id))
    }

    func chooseRoot() {
        let panel = NSOpenPanel(); panel.canChooseDirectories = true; panel.canChooseFiles = false; panel.allowsMultipleSelection = false
        panel.prompt = "Choose WoW Folder"; panel.message = "Choose World of Warcraft or a flavor folder such as _anniversary_."
        if panel.runModal() == .OK { customRoot = panel.url; refresh() }
    }

    func chooseDestination() {
        let panel = NSOpenPanel(); panel.canChooseDirectories = true; panel.canChooseFiles = false; panel.allowsMultipleSelection = false; panel.prompt = "Choose"
        if panel.runModal() == .OK, let url = panel.url { destination = url }
    }

    func openSavedVariables() {
        guard let url = accounts.first(where: { selectedAccountIDs.contains($0.id) })?.savedVariables ?? accounts.first?.savedVariables else { return }
        NSWorkspace.shared.open(url)
    }

    func openLastReport() { if let lastReport { NSWorkspace.shared.activateFileViewerSelecting([lastReport]) } }

    func createReport() {
        guard let installation = selectedInstallation else { status = "Choose a WoW installation first."; return }
        let chosen = accounts.filter { selectedAccountIDs.contains($0.id) }
        guard !chosen.isEmpty else { status = "Select at least one account."; return }
        isWorking = true; status = "Packaging addon data…"
        Task {
            do {
                saveCoachingProfile()
                let result = try await builder.create(installation: installation, accounts: chosen, destinationDirectory: destination, coachingProfile: coachingProfile)
                lastReport = result.zipURL
                status = "Created \(result.zipURL.lastPathComponent) with \(result.fileCount) files from \(result.accountCount) account\(result.accountCount == 1 ? "" : "s")."
            } catch { status = "Couldn’t create report: \(error.localizedDescription)" }
            isWorking = false
        }
    }

    var coachingProfile: CoachingProfile {
        CoachingProfile(primaryCharacter: primaryCharacter.nilIfBlank, secondaryCharacter: secondaryCharacter.nilIfBlank,
            bankOnlyCharacters: bankOnlyCharacters.split(separator: ",").map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty },
            objective: progressionObjective.nilIfBlank, targetDate: targetDate.nilIfBlank,
            rotationCharacters: rotationCharacters.split(separator: ",").map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty })
    }

    func saveCoachingProfile() {
        let defaults = UserDefaults.standard
        defaults.set(primaryCharacter, forKey: "primaryCharacter")
        defaults.set(secondaryCharacter, forKey: "secondaryCharacter")
        defaults.set(bankOnlyCharacters, forKey: "bankOnlyCharacters")
        defaults.set(progressionObjective, forKey: "progressionObjective")
        defaults.set(targetDate, forKey: "targetDate")
        defaults.set(rotationCharacters, forKey: "rotationCharacters")
    }
}

private extension String {
    var nilIfBlank: String? { trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : self }
}
