import Foundation
import Observation
import SwiftUI

@Observable
public final class Toaster: @unchecked Sendable {
    public enum Kind: Sendable {
        case info, success, warning, error
    }

    public struct Action: Identifiable, Sendable {
        public let id = UUID()
        public let title: String
        public let isDestructive: Bool
        public let handler: @MainActor @Sendable () -> Void

        public init(
            _ title: String,
            isDestructive: Bool = false,
            handler: @escaping @MainActor @Sendable () -> Void
        ) {
            self.title = title
            self.isDestructive = isDestructive
            self.handler = handler
        }
    }

    public struct Toast: Identifiable {
        public let id = UUID()
        public var kind: Kind
        public var title: String
        public var description: String?
        public var actions: [Action]
        public var isPersistent: Bool
    }

    public private(set) var toasts: [Toast] = []
    private var timers: [UUID: Task<Void, Never>] = [:]

    public init() {}

    @discardableResult
    @MainActor
    public func show(
        _ kind: Kind,
        title: String,
        description: String? = nil,
        actions: [Action] = [],
        persistent: Bool = false
    ) -> UUID {
        let toast = Toast(
            kind: kind,
            title: title,
            description: description,
            actions: actions,
            isPersistent: persistent
        )
        self.toasts.append(toast)
        if !persistent && (kind == .info || kind == .success) {
            self.scheduleDismiss(id: toast.id, after: 4)
        }
        return toast.id
    }

    @MainActor
    public func dismiss(id: UUID) {
        self.timers[id]?.cancel()
        self.timers[id] = nil
        self.toasts.removeAll { $0.id == id }
    }

    @MainActor
    public func dismissAll() {
        for timer in self.timers.values { timer.cancel() }
        self.timers.removeAll()
        self.toasts.removeAll()
    }

    @MainActor
    private func scheduleDismiss(id: UUID, after seconds: Double) {
        self.timers[id]?.cancel()
        self.timers[id] = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
            guard !Task.isCancelled else { return }
            self?.dismiss(id: id)
        }
    }
}

extension EnvironmentValues {
    @Entry public var toaster: Toaster = .init()
}
