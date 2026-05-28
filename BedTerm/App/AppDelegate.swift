import BedTermIOS
import BedTermKit
import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    private var localeObserver: NSObjectProtocol?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UITestSupport.installOverridesIfNeeded()

        // Push the active locale into the Rust i18n table once at launch,
        // then again whenever the system flips Language & Region.
        pushLocaleToRust()
        localeObserver = NotificationCenter.default.addObserver(
            forName: NSLocale.currentLocaleDidChangeNotification,
            object: nil,
            queue: .main
        ) { _ in
            MainActor.assumeIsolated {
                AppDelegate.pushLocaleToRust()
            }
        }

        return true
    }

    func application(
        _ application: UIApplication,
        configurationForConnecting connectingSceneSession: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        let cfg = UISceneConfiguration(
            name: "Default",
            sessionRole: connectingSceneSession.role
        )
        cfg.delegateClass = SceneDelegate.self
        return cfg
    }

    private func pushLocaleToRust() { Self.pushLocaleToRust() }

    /// Resolve the best Rust-table locale match for the user's preferred
    /// language and forward it to `bt_ios_set_locale`. We use
    /// `Bundle.main.preferredLocalizations.first` because it already
    /// resolves against the bundle's available locales — so a user with
    /// French preferred will land on "en" (our fallback) rather than "fr"
    /// (which isn't in our catalogue).
    private static func pushLocaleToRust() {
        let code = Bundle.main.preferredLocalizations.first ?? Locale.current.identifier
        code.withCString { bt_ios_set_locale($0) }
    }
}
