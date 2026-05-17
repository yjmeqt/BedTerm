import Foundation

/// One row in the user's saved-hosts list.
///
/// Identity is the `id` UUID — never the host/user tuple — so the user can save
/// duplicate `(host, port, username)` rows (with different labels or auth methods)
/// without the list collapsing them.
public struct SavedHost: Codable, Equatable, Identifiable {
    public let id: UUID
    public var label: String
    public var credential: HostCredential

    public init(id: UUID = UUID(), label: String, credential: HostCredential) {
        self.id = id
        self.label = label
        self.credential = credential
    }
}
