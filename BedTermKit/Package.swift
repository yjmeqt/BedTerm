// swift-tools-version:6.2
import PackageDescription

let package = Package(
    name: "BedTermKit",
    platforms: [.iOS(.v26)],
    products: [
        .library(name: "BedTermKit", targets: ["BedTermKit"])
    ],
    dependencies: [],
    targets: [
        .binaryTarget(
            name: "BedTermIOS",
            path: "BinaryFrameworks/BedTermIOS.xcframework"
        ),
        .target(
            name: "BedTermKit",
            dependencies: [
                "BedTermIOS"
            ],
            path: "Sources/BedTermKit",
            resources: [
                .process("Resources")
            ],
            linkerSettings: [
                .linkedLibrary("sqlite3"),
                // The Rust core (bedterm-ios) subclasses MTKView via the
                // ObjC runtime. Staticlibs don't propagate framework link
                // requirements, so MetalKit (and its dependency Metal)
                // must be linked explicitly here — otherwise dyld never
                // loads MetalKit into the process and
                // `objc_getClass("MTKView")` returns NULL on first
                // allocation, crashing in objc2::CachedClass::fetch.
                .linkedFramework("MetalKit"),
                .linkedFramework("Metal")
            ]
        ),
        .testTarget(
            name: "BedTermKitTests",
            dependencies: ["BedTermKit"],
            path: "Tests/BedTermKitTests",
            resources: [
                // Renderer parity tests read raw byte-stream captures from
                // disk; ship the folder verbatim so the on-disk layout the
                // tests look for is preserved.
                .copy("Fixtures")
            ]
        )
    ],
    swiftLanguageModes: [.v6]
)
