import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        NavigationSplitView {
            List(selection: Binding(get: { model.selectedInstallation?.id }, set: { id in
                model.selectedInstallation = model.installations.first { $0.id == id }; model.reloadAccounts()
            })) {
                Section("Installations") {
                    ForEach(model.installations) { item in
                        Label(item.flavor.replacingOccurrences(of: "_", with: " ").capitalized, systemImage: "gamecontroller.fill").tag(item.id)
                    }
                }
            }
            .navigationTitle("WoW Coach")
            .toolbar { Button { model.refresh() } label: { Image(systemName: "arrow.clockwise") }.help("Scan again") }
        } detail: {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    HStack(spacing: 16) {
                        Image(systemName: "wand.and.stars.inverse").font(.system(size: 34)).foregroundStyle(.purple)
                        VStack(alignment: .leading) {
                            Text("Create a coaching report").font(.largeTitle.bold())
                            Text("Your addon data stays on this Mac until you choose to share the ZIP.").foregroundStyle(.secondary)
                        }
                    }
                    GroupBox("Accounts") {
                        if model.accounts.isEmpty {
                            VStack(spacing: 8) {
                                Image(systemName: "folder.badge.questionmark").font(.title).foregroundStyle(.secondary)
                                Text("No accounts found").font(.headline)
                                Text("Open Settings and choose your World of Warcraft folder.").foregroundStyle(.secondary)
                            }.frame(maxWidth: .infinity).padding(24)
                        }
                        else { VStack(alignment: .leading) { ForEach(model.accounts) { account in Toggle(account.name, isOn: Binding(get: { model.selectedAccountIDs.contains(account.id) }, set: { on in if on { model.selectedAccountIDs.insert(account.id) } else { model.selectedAccountIDs.remove(account.id) } })) } }.padding(8) }
                    }
                    HStack {
                        Button("Create Report", systemImage: "shippingbox.fill") { model.createReport() }.buttonStyle(.borderedProminent).controlSize(.large).disabled(model.isWorking || model.accounts.isEmpty)
                        Button("Open SavedVariables", systemImage: "folder") { model.openSavedVariables() }.disabled(model.accounts.isEmpty)
                        Button("Settings", systemImage: "gearshape") { model.showingSettings = true }
                    }
                    if model.isWorking { ProgressView().controlSize(.small) }
                    Label(model.status, systemImage: model.lastReport == nil ? "info.circle" : "checkmark.circle.fill")
                        .foregroundStyle(model.lastReport == nil ? Color.secondary : Color.green)
                    if model.lastReport != nil { Button("Show report in Finder") { model.openLastReport() } }
                    GroupBox("Coaching strategy") {
                        VStack(alignment: .leading, spacing: 6) {
                            LabeledContent("Primary", value: model.primaryCharacter.isEmpty ? "Not set" : model.primaryCharacter)
                            LabeledContent("Secondary", value: model.secondaryCharacter.isEmpty ? "Not set" : model.secondaryCharacter)
                            LabeledContent("Rotation", value: model.rotationCharacters.isEmpty ? "Not set" : model.rotationCharacters)
                            LabeledContent("Goal", value: model.progressionObjective.isEmpty ? "Not set" : model.progressionObjective)
                            LabeledContent("Target", value: model.targetDate.isEmpty ? "Not set" : model.targetDate)
                        }.padding(8)
                    }
                    Divider()
                    Label("Included", systemImage: "checkmark.shield.fill").font(.headline)
                    Text("Questie, GatherMate2, Auctionator, Altoholic, DataStore modules, Pawn, SavedInstances, Bagnon, and Leatrix Lua files. Version 1.4 adds rotation-aware recommendations, estimated rested XP, trainer and talent reminders, known profession recipes, expanded materials, and Auction House scan metadata.").foregroundStyle(.secondary)
                }.padding(32).frame(maxWidth: 760, alignment: .leading)
            }
        }
        .frame(minWidth: 850, minHeight: 590)
        .sheet(isPresented: $model.showingSettings) { SettingsView() }
    }
}

struct SettingsView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Settings").font(.title.bold())
            LabeledContent("WoW folder") { HStack { Text(model.customRoot?.path ?? "Automatic").lineLimit(1); Button("Choose…") { model.chooseRoot() } } }
            LabeledContent("Report destination") { HStack { Text(model.destination.path).lineLimit(1); Button("Choose…") { model.chooseDestination() } } }
            Divider()
            Text("Coaching strategy").font(.headline)
            Form {
                TextField("Primary character", text: $model.primaryCharacter)
                TextField("Secondary character", text: $model.secondaryCharacter)
                TextField("Bank-only characters", text: $model.bankOnlyCharacters)
                TextField("Active rotation (in order)", text: $model.rotationCharacters)
                TextField("Objective", text: $model.progressionObjective)
                TextField("Target date", text: $model.targetDate)
            }
            Text("Tip: fully log out or use /reload before creating a report so addons save their latest state.").foregroundStyle(.secondary)
            Spacer()
            HStack { Spacer(); Button("Done") { model.saveCoachingProfile(); dismiss() }.keyboardShortcut(.defaultAction) }
        }.padding(28).frame(width: 680, height: 500)
    }
}
