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

// MARK: - Step 3a: macOS Tutorial (paged)

private struct MacTutorialStep: View {
    let onContinue: () -> Void

    @State private var currentStep = 0
    private let stepCount = 3

    var body: some View {
        VStack(spacing: 0) {
            TabView(selection: $currentStep) {
                macTutorialPage(
                    step: 1,
                    title: "Enable Remote Login",
                    body: "On your Mac, open System Settings → General → Sharing, then turn on Remote Login."
                )
                .tag(0)

                macTutorialPage(
                    step: 2,
                    title: "Find your Mac's IP address",
                    body:
                        "Open System Settings → Network → Wi-Fi → Details. "
                        + "Copy the IP address — you'll enter it on the next screen."
                )
                .tag(1)

                sleepPreventionPage
                    .tag(2)
            }
            .tabViewStyle(.page(indexDisplayMode: .never))

            bottomBar
        }
        .navigationTitle("macOS Setup")
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: Bottom Bar

    private var bottomBar: some View {
        HStack {
            HStack(spacing: 6) {
                ForEach(0..<stepCount, id: \.self) { index in
                    Circle()
                        .fill(index == currentStep ? Color.accentColor : Color.secondary.opacity(0.3))
                        .frame(width: 8, height: 8)
                }
            }

            Spacer()

            Button {
                if currentStep < stepCount - 1 {
                    withAnimation { currentStep += 1 }
                } else {
                    onContinue()
                }
            } label: {
                Text(currentStep == stepCount - 1 ? "Done" : "Continue")
                    .padding(.horizontal, 24)
                    .padding(.vertical, 4)
            }
            .buttonStyle(.borderedProminent)
            .accessibilityIdentifier("onboarding.macTutorial.continue")
        }
        .padding(.horizontal)
        .padding(.vertical, 12)
        .background(.bar)
    }

    // MARK: Shared Page Layout

    private func macTutorialPage(step: Int, title: LocalizedStringKey, body: LocalizedStringKey) -> some View {
        VStack(alignment: .leading, spacing: 24) {
            Spacer()
            tutorialSection(number: String(step), title: title, body: body)
            Spacer()
        }
        .padding()
    }

    // MARK: Step 3: Prevent Sleep

    private var sleepPreventionPage: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(alignment: .top, spacing: 12) {
                    Text("3")
                        .font(.title2.bold())
                        .frame(width: 28, height: 28)
                        .background(Circle().fill(Color.accentColor.opacity(0.15)))
                        .foregroundStyle(Color.accentColor)

                    VStack(alignment: .leading, spacing: 12) {
                        Text("Prevent your Mac from sleeping").font(.headline)
                        Text(
                            "A sleeping Mac silently drops SSH connections. "
                                + "Your display can sleep — the system must stay awake."
                        )
                        .font(.body)
                        .foregroundStyle(.secondary)

                        VStack(alignment: .leading, spacing: 4) {
                            Text("System Settings")
                                .font(.subheadline.weight(.semibold))
                            Text(
                                "Battery → Options → turn on "
                                    + "\"Prevent automatic sleeping on power adapter"
                                    + " when the display is off\""
                            )
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                        }

                        VStack(alignment: .leading, spacing: 6) {
                            Text("Or run this in Terminal:")
                                .font(.subheadline.weight(.semibold))

                            CopyableCodeBlock(command: "sudo pmset -a sleep 0")
                        }

                        VStack(alignment: .leading, spacing: 6) {
                            Text("To undo, return to the same Battery setting, or run:")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)

                            CopyableCodeBlock(command: "sudo pmset -a sleep 1")
                        }
                    }
                }
            }
            .padding()
        }
    }

    // MARK: Shared Helpers

    private func tutorialSection(number: String, title: LocalizedStringKey, body: LocalizedStringKey) -> some View {
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
        .navigationTitle(navTitle)
        .navigationBarTitleDisplayMode(.inline)
    }

    private var navTitle: String {
        if viewModel.location == .sameWifi {
            return String(localized: "Permission")
        }
        return String(localized: "Done")
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
