import BedTermCoreC
import UIKit

/// UIKit shell that hosts the Rust-backed terminal VC as a direct child
/// of a `UINavigationController`. The Rust VC owns its own connection-state
/// UI (HUD + glyph renderer), so no SwiftUI overlays are layered here —
/// only the back navigation bar item lives in Swift. Tapping back ends the
/// live SSH session (no separate kill button — Back == End session).
///
/// This replaces the `IosTerminalView` SwiftUI representable that used to
/// wrap the Rust VC inside a SwiftUI `NavigationStack` destination —
/// pushing this VC onto the nav controller avoids the extra SwiftUI
/// containment layer and keeps the responder chain rooted at `UIWindow
/// → UINavigationController → TerminalScreenViewController → Rust VC`.
@MainActor
final class TerminalScreenViewController: UIViewController {
    private let host: IosTerminalHost
    private let credential: HostCredential
    private let hostName: String
    private let onBack: () -> Void
    private let bootstrapPayloadProvider: @MainActor () -> String?

    init(
        session: TerminalSession,
        credential: HostCredential,
        hostName: String,
        bootstrapPayloadProvider: @escaping @MainActor () -> String?,
        onBack: @escaping () -> Void
    ) {
        self.host = IosTerminalHost(session: session)
        self.credential = credential
        self.hostName = hostName
        self.bootstrapPayloadProvider = bootstrapPayloadProvider
        self.onBack = onBack
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(named: "ShadcnBackground", in: .module, compatibleWith: nil)
        installRustVC()
        installNavigationBarItems()
        Task { @MainActor in
            await connectIfNeeded()
        }
    }

    deinit {
        // Final cleanup — `viewDidDisappear` already tore the host down for
        // the normal flow, but if the VC is dropped without ever appearing
        // (e.g. cancelled push) this catches it.
        let host = self.host
        Task { @MainActor in
            host.teardown()
        }
    }

    // MARK: - Setup

    private func installRustVC() {
        let vc = host.makeVC { [weak self] in
            self?.onBack()
        }
        addChild(vc)
        vc.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(vc.view)
        NSLayoutConstraint.activate([
            vc.view.topAnchor.constraint(equalTo: view.topAnchor),
            vc.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            vc.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            vc.view.trailingAnchor.constraint(equalTo: view.trailingAnchor)
        ])
        vc.didMove(toParent: self)
    }

    private func installNavigationBarItems() {
        navigationItem.largeTitleDisplayMode = .never
        let titleLabel = UILabel()
        titleLabel.text = hostName
        titleLabel.font = .preferredFont(forTextStyle: .subheadline).withWeightSemibold()
        titleLabel.textColor = UIColor(named: "ShadcnPrimary", in: .module, compatibleWith: nil)
        titleLabel.lineBreakMode = .byTruncatingMiddle
        navigationItem.titleView = titleLabel

        let back = UIBarButtonItem(
            image: UIImage(systemName: "chevron.left"),
            style: .plain,
            target: self,
            action: #selector(backTapped)
        )
        back.accessibilityLabel = String(localized: "Back")
        back.accessibilityIdentifier = "terminal.back"

        navigationItem.leftBarButtonItem = back
        navigationItem.hidesBackButton = true
    }

    @objc private func backTapped() {
        onBack()
    }

    // MARK: - Connect

    private func connectIfNeeded() async {
        guard case .idle = host.session.state else { return }
        let initial =
            await host.awaitInitialGridDim(timeoutMs: 500)
            ?? PTYDimensions(cols: 80, rows: 24)
        await host.session.connect(
            credential: credential,
            initialPTY: initial,
            bootstrapPayload: bootstrapPayloadProvider()
        )
    }

    // MARK: - Lifecycle

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        // If we're being popped off the nav stack (not just temporarily
        // covered by a presented sheet), tear down the Rust VC.
        if isMovingFromParent || isBeingDismissed {
            host.teardown()
        }
    }
}

private extension UIFont {
    func withWeightSemibold() -> UIFont {
        let descriptor = fontDescriptor.addingAttributes([
            .traits: [UIFontDescriptor.TraitKey.weight: UIFont.Weight.semibold]
        ])
        return UIFont(descriptor: descriptor, size: pointSize)
    }
}
