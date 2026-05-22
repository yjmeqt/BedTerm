import BedTermKit
import SwiftUI

@main
struct BedTermApp: App {
    @State private var path = NavigationPath()
    @State private var toaster = Toaster()
    @State private var settings = BedTermSettings()
    @State private var persistenceHandle: PersistenceHandle?
    @State private var persistedSnapshots: PersistedSessionSnapshotStore?

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
            .task {
                if persistenceHandle == nil {
                    let dbURL = URL.applicationSupportDirectory.appending(path: "BedTerm/sessions.sqlite")
                    persistenceHandle = PersistenceHandle.open(at: dbURL)
                }
                if persistedSnapshots == nil, let handle = persistenceHandle {
                    persistedSnapshots = PersistedSessionSnapshotStore(handle: handle)
                }
            }
        }
    }
}
