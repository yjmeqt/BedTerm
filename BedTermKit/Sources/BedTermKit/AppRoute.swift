import Foundation

enum AppRoute: Hashable {
    /// Push the add/edit form. `nil` id means "add a new host"; a non-nil id
    /// means "edit the existing entry with this id" (resolved by the form screen
    /// from `HostsStore`).
    case hostForm(SavedHost.ID?)
    case terminal
}
