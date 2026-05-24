import BedTermKit
import SwiftUI

@main
struct BedTermApp: App {
    @State private var path = NavigationPath()
    @State private var toaster = Toaster()
    @State private var settings = BedTermSettings()
    @State private var persistenceHandle: PersistenceHandle?
    /// Default to a no-op store; attach the SQLite handle when it opens.
    @State private var persistedSnapshots = PersistedSessionSnapshotStore(handle: nil)

    @State private var onboardingDone: Bool =
        OnboardingViewModel.hasCompleted
        || ProcessInfo.processInfo.arguments.contains("-uitest-skipOnboarding")

    var body: some Scene {
        WindowGroup {
            navigationRoot
                .environment(\.toaster, toaster)
                .environment(settings)
                .environment(\.persistenceHandle, persistenceHandle)
                .environment(persistedSnapshots)
                .task {
                    if persistenceHandle == nil {
                        let dbURL = URL.applicationSupportDirectory.appending(
                            path: "BedTerm/sessions.sqlite")
                        // SQLite open can stall on WAL recovery or a stale
                        // file lock after a prior force-quit. Run it off the
                        // main actor so the HostsScreen stays interactive
                        // even if the open never returns.
                        let handle = await Task.detached(priority: .userInitiated) {
                            PersistenceHandle.open(at: dbURL)
                        }.value
                        if let handle {
                            persistedSnapshots.attach(handle: handle)
                            persistenceHandle = handle
                        }
                    }
                }
        }
    }

    @ViewBuilder
    private var navigationRoot: some View {
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
    }
}
