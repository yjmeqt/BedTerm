import Foundation

/// Companion file to `HostsViewModel.swift` — public state structs that
/// the @Observable VM exposes, plus the JSON decoders that bridge them
/// across the FFI boundary with the Rust-side `hosts_vm` mirror.
///
/// These were nested inside `HostsViewModel` pre-port; lifted to the
/// top level so SwiftLint's `nesting` rule (1-level cap) is satisfied
/// once `CodingKeys` lives inside each struct.

public struct HostsPendingMismatch: Equatable, Codable {
    public let stored: String
    public let remote: String
    public let host: String
    public let port: Int
    public let sourceID: UUID

    enum CodingKeys: String, CodingKey {
        case stored, remote, host, port, sourceID
    }

    public init(stored: String, remote: String, host: String, port: Int, sourceID: UUID) {
        self.stored = stored
        self.remote = remote
        self.host = host
        self.port = port
        self.sourceID = sourceID
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.stored = try container.decode(String.self, forKey: .stored)
        self.remote = try container.decode(String.self, forKey: .remote)
        self.host = try container.decode(String.self, forKey: .host)
        self.port = try container.decode(Int.self, forKey: .port)
        let idString = try container.decode(String.self, forKey: .sourceID)
        guard let uuid = UUID(uuidString: idString) else {
            throw DecodingError.dataCorruptedError(
                forKey: .sourceID, in: container,
                debugDescription: "sourceID is not a UUID")
        }
        self.sourceID = uuid
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(stored, forKey: .stored)
        try container.encode(remote, forKey: .remote)
        try container.encode(host, forKey: .host)
        try container.encode(port, forKey: .port)
        try container.encode(sourceID.uuidString, forKey: .sourceID)
    }
}

public struct HostsSwapConfirmation: Equatable, Identifiable, Codable {
    public let targetID: UUID
    public let displayName: String
    public var id: UUID { self.targetID }

    enum CodingKeys: String, CodingKey { case targetID, displayName }

    public init(targetID: UUID, displayName: String) {
        self.targetID = targetID
        self.displayName = displayName
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let idString = try container.decode(String.self, forKey: .targetID)
        guard let uuid = UUID(uuidString: idString) else {
            throw DecodingError.dataCorruptedError(
                forKey: .targetID, in: container,
                debugDescription: "targetID is not a UUID")
        }
        self.targetID = uuid
        self.displayName = try container.decode(String.self, forKey: .displayName)
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(targetID.uuidString, forKey: .targetID)
        try container.encode(displayName, forKey: .displayName)
    }
}

public struct HostsDeleteConfirmation: Equatable, Identifiable, Codable {
    public let targetID: UUID
    public let displayName: String
    public let isLive: Bool
    public let isInFlight: Bool
    public var id: UUID { self.targetID }

    enum CodingKeys: String, CodingKey { case targetID, displayName, isLive, isInFlight }

    public init(targetID: UUID, displayName: String, isLive: Bool, isInFlight: Bool) {
        self.targetID = targetID
        self.displayName = displayName
        self.isLive = isLive
        self.isInFlight = isInFlight
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let idString = try container.decode(String.self, forKey: .targetID)
        guard let uuid = UUID(uuidString: idString) else {
            throw DecodingError.dataCorruptedError(
                forKey: .targetID, in: container,
                debugDescription: "targetID is not a UUID")
        }
        self.targetID = uuid
        self.displayName = try container.decode(String.self, forKey: .displayName)
        self.isLive = try container.decode(Bool.self, forKey: .isLive)
        self.isInFlight = try container.decode(Bool.self, forKey: .isInFlight)
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(targetID.uuidString, forKey: .targetID)
        try container.encode(displayName, forKey: .displayName)
        try container.encode(isLive, forKey: .isLive)
        try container.encode(isInFlight, forKey: .isInFlight)
    }
}

extension HostsViewModel {
    public typealias PendingMismatch = HostsPendingMismatch
    public typealias SwapConfirmation = HostsSwapConfirmation
    public typealias DeleteConfirmation = HostsDeleteConfirmation
}

/// Decoder-only view of the Rust entries snapshot. `HostsViewModel`
/// resolves each id back to its full `SavedHost` via the local store
/// so callers still see Swift-typed credentials.
struct HostsEntryRef: Decodable {
    let id: UUID

    enum CodingKeys: String, CodingKey { case id }
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let idString = try container.decode(String.self, forKey: .id)
        guard let uuid = UUID(uuidString: idString) else {
            throw DecodingError.dataCorruptedError(
                forKey: .id, in: container, debugDescription: "id is not a UUID")
        }
        self.id = uuid
    }
}
