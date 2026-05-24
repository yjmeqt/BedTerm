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
                    Toggle(isOn: $settings.showCommandBlocks) {
                        VStack(alignment: .leading, spacing: 2) {
                            HStack(spacing: 6) {
                                Text("Command blocks")
                                Text("Beta")
                                    .font(.caption2.weight(.semibold))
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(
                                        Capsule().fill(Color("ShadcnBorder", bundle: .module))
                                    )
                                    .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                            }
                            Text(
                                """
                                Show each command and its output as a separate \
                                block (Warp-style). When on, BedTerm writes a \
                                small shell-integration script into every new SSH \
                                session to track prompt and command boundaries. \
                                Without shell integration, \
                                blocks won't show command metadata. \
                                BedTerm uses Warp's DCS hook protocol, not \
                                OSC 133, so third-party integrations won't \
                                drive it. This feature is still in beta and off \
                                by default.
                                """
                            )
                            .font(.footnote)
                            .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                        }
                    }
                    .accessibilityIdentifier("settings.commandBlocks")
                } header: {
                    Text("Blocks")
                }

                #if DEBUG
                    Section {
                        Toggle(isOn: $settings.useRustTerminal) {
                            VStack(alignment: .leading, spacing: 2) {
                                HStack(spacing: 6) {
                                    Text("Rust terminal (experimental)")
                                    Text("Exp")
                                        .font(.caption2.weight(.semibold))
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 2)
                                        .background(
                                            Capsule().fill(Color("ShadcnBorder", bundle: .module))
                                        )
                                        .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                                }
                                Text(
                                    """
                                    Route the terminal screen through an experimental \
                                    Rust-backed view controller. Off by default.
                                    """
                                )
                                .font(.footnote)
                                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
                            }
                        }
                        .accessibilityIdentifier("settings.useRustTerminal")
                    } header: {
                        Text("Experimental")
                    }
                #endif
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
