import SwiftUI

/// Root onboarding view (R10). A two-step picker (host kind → location) leads
/// to either a macOS setup tutorial or a Local Network permission prompt, then
/// hands off to the Connection form via `onFinish`.
public struct OnboardingScreen: View {
    let onFinish: () -> Void

    @State private var viewModel = OnboardingViewModel()
    @State private var path: [OnboardingViewModel.Step] = []

    public init(onFinish: @escaping () -> Void) {
        self.onFinish = onFinish
    }

    public var body: some View {
        NavigationStack(path: $path) {
            HostKindStep(viewModel: viewModel) {
                path.append(.location)
            }
            .navigationDestination(for: OnboardingViewModel.Step.self) { step in
                switch step {
                case .location:
                    LocationStep(viewModel: viewModel) {
                        path.append(viewModel.nextStepAfterLocation)
                    }
                case .macTutorial:
                    MacTutorialStep {
                        if viewModel.location == .sameWifi {
                            path.append(.localPermission)
                        } else {
                            finish()
                        }
                    }
                case .localPermission:
                    LocalPermissionStep(viewModel: viewModel) {
                        finish()
                    }
                }
            }
        }
    }

    private func finish() {
        viewModel.finish()
        onFinish()
    }
}

// MARK: - Step 1: Host Kind

private struct HostKindStep: View {
    @Bindable var viewModel: OnboardingViewModel
    let onNext: () -> Void

    var body: some View {
        VStack(spacing: 24) {
            Spacer()
            Text("Welcome to BedTerm")
                .font(.largeTitle.bold())
            Text("What kind of host will you connect to?")
                .font(.title3)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal)

            VStack(spacing: 12) {
                Button {
                    viewModel.selectHostKind(.macOS)
                    onNext()
                } label: {
                    OnboardingChoiceLabel(
                        title: "macOS",
                        subtitle: "A Mac mini, MacBook, or iMac on your network"
                    )
                }
                .accessibilityIdentifier("onboarding.hostKind.macos")

                Button {
                    viewModel.selectHostKind(.other)
                    onNext()
                } label: {
                    OnboardingChoiceLabel(
                        title: "Linux / other",
                        subtitle: "Linux box, VM, Raspberry Pi, cloud server"
                    )
                }
                .accessibilityIdentifier("onboarding.hostKind.other")
            }
            .buttonStyle(.plain)
            .padding(.horizontal)

            Spacer()
        }
        .navigationTitle("Setup")
        .navigationBarTitleDisplayMode(.inline)
    }
}

// MARK: - Step 2: Location

private struct LocationStep: View {
    @Bindable var viewModel: OnboardingViewModel
    let onNext: () -> Void

    var body: some View {
        VStack(spacing: 24) {
            Spacer()
            Text("Where is the host?")
                .font(.title.bold())
            Text("This determines whether we need Local Network access.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal)

            VStack(spacing: 12) {
                Button {
                    viewModel.selectLocation(.sameWifi)
                    onNext()
                } label: {
                    OnboardingChoiceLabel(
                        title: "Same Wi-Fi as my phone",
                        subtitle: "iOS will ask for Local Network permission"
                    )
                }
                .accessibilityIdentifier("onboarding.location.sameWifi")

                Button {
                    viewModel.selectLocation(.remote)
                    onNext()
                } label: {
                    OnboardingChoiceLabel(
                        title: "Remote (over the internet)",
                        subtitle: "A public IP or hostname reachable from anywhere"
                    )
                }
                .accessibilityIdentifier("onboarding.location.remote")
            }
            .buttonStyle(.plain)
            .padding(.horizontal)

            Spacer()
        }
        .navigationTitle("Where")
        .navigationBarTitleDisplayMode(.inline)
    }
}

// MARK: - Step 3a: macOS Tutorial (placeholder copy — content TBD)

private struct MacTutorialStep: View {
    let onContinue: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text("Set up your Mac")
                    .font(.title.bold())

                tutorialSection(
                    number: "1",
                    title: "Enable Remote Login",
                    body:
                        """
                        On your Mac, open System Settings → General → Sharing, then turn on Remote Login. \
                        (Detailed walkthrough coming soon.)
                        """
                )

                tutorialSection(
                    number: "2",
                    title: "Find your Mac's IP address",
                    body:
                        """
                        Open System Settings → Network → Wi-Fi → Details. \
                        Copy the IP address — you'll enter it on the next screen. \
                        (Detailed walkthrough coming soon.)
                        """
                )

            }
            .padding()
        }
        .safeAreaInset(edge: .bottom) {
            Button(action: onContinue) {
                Text("Continue")
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 4)
            }
            .buttonStyle(.borderedProminent)
            .padding()
            .accessibilityIdentifier("onboarding.macTutorial.continue")
        }
        .navigationTitle("macOS Setup")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func tutorialSection(number: String, title: String, body: String) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Text(number)
                .font(.title2.bold())
                .frame(width: 28, height: 28)
                .background(Circle().fill(Color.accentColor.opacity(0.15)))
                .foregroundStyle(Color.accentColor)
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(.headline)
                Text(body).font(.body).foregroundStyle(.secondary)
            }
        }
    }
}

// MARK: - Step 3b: Local Network Permission

private struct LocalPermissionStep: View {
    @Bindable var viewModel: OnboardingViewModel
    let onContinue: () -> Void

    var body: some View {
        VStack(spacing: 20) {
            Spacer()
            Image(systemName: "wifi")
                .font(.system(size: 56))
                .foregroundStyle(.tint)
            Group {
                if viewModel.location == .sameWifi {
                    Text("Local Network Access")
                } else {
                    Text("All set")
                }
            }
            .font(.title.bold())
            Text(detailText)
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
                .padding(.horizontal)
            Spacer()

            switch viewModel.permissionResult {
            case .denied:
                Text("Permission denied. You can enable it later in Settings → BedTerm.")
                    .font(.footnote)
                    .foregroundStyle(.red)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal)
            default:
                EmptyView()
            }

            Button(action: triggerOrFinish) {
                Group {
                    if viewModel.isRequestingPermission {
                        ProgressView()
                    } else {
                        Text(buttonTitle)
                    }
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 4)
            }
            .buttonStyle(.borderedProminent)
            .disabled(viewModel.isRequestingPermission)
            .padding(.horizontal)
            .padding(.bottom)
            .accessibilityIdentifier("onboarding.localPermission.action")
        }
        .navigationTitle(viewModel.location == .sameWifi
            ? String(localized: "Permission")
            : String(localized: "Done"))
        .navigationBarTitleDisplayMode(.inline)
    }

    private var detailText: String {
        if viewModel.location == .sameWifi {
            return String(
                localized:
                    "BedTerm needs Local Network access to reach your Mac on the same Wi-Fi. Tap Allow when iOS prompts you."
            )
        }
        return String(
            localized:
                "You're ready to connect to a remote host. Tap Continue to enter the connection details."
        )
    }

    private var buttonTitle: String {
        if viewModel.location != .sameWifi { return String(localized: "Continue") }
        switch viewModel.permissionResult {
        case .none: return String(localized: "Request Permission")
        case .granted, .unknown: return String(localized: "Continue")
        case .denied: return String(localized: "Continue Anyway")
        }
    }

    private func triggerOrFinish() {
        if viewModel.location != .sameWifi || viewModel.permissionResult != nil {
            onContinue()
            return
        }
        Task {
            await viewModel.requestLocalNetworkIfNeeded()
        }
    }
}

// MARK: - Shared

private struct OnboardingChoiceLabel: View {
    let title: String
    let subtitle: String

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.headline)
                Text(subtitle).font(.subheadline).foregroundStyle(.secondary)
            }
            Spacer()
            Image(systemName: "chevron.right").foregroundStyle(.tertiary)
        }
        .padding()
        .background(
            RoundedRectangle(cornerRadius: 14)
                .fill(.regularMaterial)
        )
    }
}
