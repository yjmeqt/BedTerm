import BedTermKit
import SwiftUI

@main
struct BedTermApp: App {
    @State private var path = NavigationPath()
    @State private var toaster = Toaster()
    @State private var settings = BedTermSettings()
    @State private var sessionSnapshots = SessionSnapshotStore()
    @State private var onboardingDone: Bool =
        OnboardingViewModel.hasCompleted
        || ProcessInfo.processInfo.arguments.contains("-uitest-skipOnboarding")

    var body: some Scene {
        WindowGroup {
            ZStack(alignment: .top) {
                Group {
                    if onboardingDone {
                        NavigationStack(path: $path) {
                            HostsScreen(path: $path)
                        }
                    } else {
                        OnboardingScreen {
                            onboardingDone = true
                        }
                    }
                }
                ToasterOverlay()
            }
            .environment(\.toaster, toaster)
            .environment(settings)
            .environment(sessionSnapshots)
        }
    }
}
