import Foundation

struct WoWDiscovery: Sendable {
    static let flavorFolders = ["_anniversary_", "_classic_", "_classic_era_", "_retail_"]

    func installations(customRoot: URL? = nil) -> [WoWInstallation] {
        var roots: [URL] = []
        if let customRoot { roots.append(customRoot) }
        roots += [
            URL(fileURLWithPath: "/Applications/World of Warcraft"),
            FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Applications/World of Warcraft")
        ]
        var found: [WoWInstallation] = []
        for root in roots {
            if root.lastPathComponent.hasPrefix("_") {
                let saved = root.appendingPathComponent("WTF/Account")
                if directoryExists(saved) { found.append(.init(root: root, flavor: root.lastPathComponent)) }
            }
            for flavor in Self.flavorFolders {
                let candidate = root.appendingPathComponent(flavor)
                if directoryExists(candidate.appendingPathComponent("WTF/Account")) {
                    found.append(.init(root: candidate, flavor: flavor))
                }
            }
        }
        return Array(Set(found)).sorted { $0.flavor < $1.flavor }
    }

    func accounts(in installation: WoWInstallation) -> [AccountSource] {
        let accountRoot = installation.root.appendingPathComponent("WTF/Account")
        let keys: Set<URLResourceKey> = [.isDirectoryKey, .isSymbolicLinkKey, .nameKey]
        guard let children = try? FileManager.default.contentsOfDirectory(at: accountRoot, includingPropertiesForKeys: Array(keys), options: [.skipsHiddenFiles]) else { return [] }
        return children.compactMap { url in
            guard let values = try? url.resourceValues(forKeys: keys), values.isDirectory == true, values.isSymbolicLink != true else { return nil }
            let saved = url.appendingPathComponent("SavedVariables")
            guard directoryExists(saved) else { return nil }
            return AccountSource(name: url.lastPathComponent, savedVariables: saved)
        }.sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    private func directoryExists(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
    }
}
