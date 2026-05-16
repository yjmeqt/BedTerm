import BedTermKit
import SwiftUI

@main
struct BedTermApp: App {
    @State private var path = NavigationPath()
    @State private var onboardingDone: Bool =
        OnboardingViewModel.hasCompleted
        || ProcessInfo.processInfo.arguments.contains("-uitest-skipOnboarding")

    var body: some Scene {
        WindowGroup {
            Group {
                if onboardingDone {
                    NavigationStack(path: $path) {
                        ConnectionScreen(path: $path)
                    }
                } else {
                    OnboardingScreen {
                        onboardingDone = true
                    }
                }
            }
            .preferredColorScheme(.dark)
        }
    }
}
