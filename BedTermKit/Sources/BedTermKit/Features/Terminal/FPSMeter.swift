#if DEBUG
    import Observation
    import QuartzCore
    import UIKit

    /// DEBUG-only FPS sampler driven by `CADisplayLink`. Counts frames over a
    /// one-second window and publishes the rolling rate via `@Observable`
    /// so the geom HUD can read it inline. Lifecycle is automatic: invalidate
    /// the link when the meter goes out of scope.
    @MainActor
    @Observable
    final class FPSMeter {
        private(set) var fps: Int = 0
        private var displayLink: CADisplayLink?
        private var frameCount: Int = 0
        private var windowStart: CFTimeInterval = 0

        init() {
            let proxy = DisplayLinkProxy { [weak self] link in
                MainActor.assumeIsolated { self?.tick(link) }
            }
            let link = CADisplayLink(target: proxy, selector: #selector(DisplayLinkProxy.fire(_:)))
            link.add(to: .main, forMode: .common)
            self.displayLink = link
        }

        isolated deinit {
            displayLink?.invalidate()
        }

        private func tick(_ link: CADisplayLink) {
            if windowStart == 0 {
                windowStart = link.timestamp
            }
            frameCount += 1
            let elapsed = link.timestamp - windowStart
            if elapsed >= 1.0 {
                fps = Int((Double(frameCount) / elapsed).rounded())
                frameCount = 0
                windowStart = link.timestamp
            }
        }
    }

    /// Tiny NSObject shim so `CADisplayLink`'s ObjC target/selector API can
    /// drive a Swift closure without retaining the meter directly (the link
    /// would otherwise keep the meter alive forever, since it itself is
    /// retained by the runloop).
    private final class DisplayLinkProxy: NSObject {
        private let onTick: (CADisplayLink) -> Void
        init(_ onTick: @escaping (CADisplayLink) -> Void) { self.onTick = onTick }
        @objc func fire(_ link: CADisplayLink) { onTick(link) }
    }
#endif
