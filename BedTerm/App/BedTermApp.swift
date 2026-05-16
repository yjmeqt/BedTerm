import SwiftUI

@main
struct BedTermApp: App {
    @State private var path = NavigationPath()

    var body: some Scene {
        WindowGroup {
            NavigationStack(path: $path) {
                ConnectionScreen(path: $path)
            }
            .preferredColorScheme(.dark)
        }
    }
}
