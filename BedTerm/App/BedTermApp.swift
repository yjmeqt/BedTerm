import BedTermKit
import SwiftUI

@main
struct BedTermApp: App {
    @State private var path = NavigationPath()
    @State private var toaster = Toaster()
    @State private var settings = BedTermSettings()
    @State private var persistenceHandle: PersistenceHandle?
    @State private var persistedSnapshots: PersistedSessionSnapshotStore?

    @State private var onboardingDone: Bool =
        OnboardingViewModel.hasCompleted
        || ProcessInfo.processInfo.arguments.contains("-uitest-skipOnboarding")

    var body: some Scene {
        WindowGroup {
            rootContent
                .environment(\.toaster, toaster)
                .environment(settings)
                .environment(\.persistenceHandle, persistenceHandle)
                .task {
                    if persistenceHandle == nil {
                        let dbURL = URL.applicationSupportDirectory.appending(
                            path: "BedTerm/sessions.sqlite")
                        persistenceHandle = PersistenceHandle.open(at: dbURL)
                    }
                    if persistedSnapshots == nil, let handle = persistenceHandle {
                        persistedSnapshots = PersistedSessionSnapshotStore(handle: handle)
                    }
                }
        }
    }

    /// Injects `PersistedSessionSnapshotStore` once SQLite is open (usually
    /// < 50 ms). Until then a transient `ProgressView` is shown — barely
    /// visible in practice since SQLite open is synchronous.
    @ViewBuilder
    private var rootContent: some View {
        if let store = persistedSnapshots {
            navigationRoot
                .environment(store)
        } else {
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
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
