import Foundation

enum AppRoute: Hashable {
    case terminal
    case hostKeyMismatch(stored: String, remote: String, host: String, port: Int)
}
