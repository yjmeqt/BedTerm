import SwiftUI

struct HostKeyMismatchScreen: View {
    let stored: String
    let remote: String
    let host: String
    let port: Int
    let onTrust: () -> Void
    let onReject: () -> Void

    var body: some View {
        Text("Host key mismatch — filled in next task.")
    }
}
