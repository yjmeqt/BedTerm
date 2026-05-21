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
                    Toggle(isOn: $settings.reserveTopSafeAreaInAltScreen) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Keep first row visible in full-screen apps")
                            // Backslash continuations keep this a single
                            // localizable literal — Swift folds the trailing
                            // \-newline away, leaving a one-line string that
                            // exactly matches the Localizable.xcstrings key.
                            Text(
                                """
                                When vim, htop, claude or other full-screen tools \
                                run, reserve the top safe area so the Dynamic \
                                Island, notch, or status bar doesn't cover their \
                                first row.
                                """
                            )
                            .font(.footnote)
                            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        }
                    }
                    .accessibilityIdentifier("settings.reserveTopSafeArea")
                } header: {
                    Text("Display")
                }

                Section {
                    Toggle(isOn: $settings.installShellIntegrationOnConnect) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Install shell integration on connect")
                            Text(
                                """
                                Push a small zsh / bash snippet into each new \
                                SSH session so prompt and command boundaries \
                                are reported back (OSC 133). Required for \
                                upcoming Block view; harmless if unused.
                                """
                            )
                            .font(.footnote)
                            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        }
                    }
                    .accessibilityIdentifier("settings.installShellIntegration")
                } header: {
                    Text("Shell integration")
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
