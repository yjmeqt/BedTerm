import SwiftUI

/// Modal settings sheet reached from the Hosts toolbar. Intentionally minimal —
/// just the toggles owned by `BedTermSettings`. New settings are added by
/// extending `BedTermSettings` and the Form below.
struct SettingsScreen: View {
    @Environment(BedTermSettings.self) private var settings
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        @Bindable var settings = settings
        NavigationStack {
            Form {
                Section {
                    Toggle(isOn: $settings.autoHideComposerInAltScreen) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Auto-hide composer in full-screen apps")
                            Text("Hide the input bar when vim, claude, htop and other full-screen tools take over the terminal.")
                                .font(.footnote)
                                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        }
                    }
                    .accessibilityIdentifier("settings.autoHideComposer")
                } header: {
                    Text("Composer")
                }

                Section {
                    Toggle(isOn: .constant(false)) {
                        VStack(alignment: .leading, spacing: 2) {
                            HStack(spacing: 6) {
                                Text("Command blocks")
                                Text("Coming soon")
                                    .font(.caption2.weight(.medium))
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(Color("ShadcnCard", bundle: .module))
                                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                                    .clipShape(Capsule())
                            }
                            Text("Group command output into collapsible blocks once shell integration ships.")
                                .font(.footnote)
                                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        }
                    }
                    .disabled(true)
                    .accessibilityIdentifier("settings.commandBlocks")
                } header: {
                    Text("Blocks")
                }
            }
            .navigationTitle(Text("Settings"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button(String(localized: "Done")) { dismiss() }
                }
            }
        }
    }
}
