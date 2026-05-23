// swift-tools-version:6.2
import PackageDescription

let package = Package(
    name: "BedTermKit",
    platforms: [.iOS(.v26)],
    products: [
        .library(name: "BedTermKit", targets: ["BedTermKit"])
    ],
    dependencies: [
        .package(url: "https://github.com/orlandos-nl/Citadel", from: "0.7.0"),
        .package(url: "https://github.com/realm/SwiftLint", from: "0.57.0")
    ],
    targets: [
        .binaryTarget(
            name: "BedTermCore",
            path: "BinaryFrameworks/BedTermCore.xcframework"
        ),
        // C header wrapper so Swift targets can `import BedTermCoreC`.
        // The actual symbols live in BedTermCore (the .a xcframework).
        // rusqlite (inside BedTermCore) dynamically links against the
        // system SQLite, so we declare that dependency here so Xcode
        // passes -lsqlite3 when linking any target that depends on us.
        .target(
            name: "BedTermCoreC",
            dependencies: ["BedTermCore"],
            path: "Sources/BedTermCoreC",
            publicHeadersPath: "include",
            linkerSettings: [
                .linkedLibrary("sqlite3")
            ]
        ),
        .target(
            name: "BedTermKit",
            dependencies: [
                "BedTermCoreC",
                .product(name: "Citadel", package: "Citadel")
            ],
            path: "Sources/BedTermKit",
            resources: [
                .process("Resources"),
                // Shell-integration scripts ship as opaque resources — SPM's
                // `.process` ignores `.sh` so `Bundle.module` wouldn't find
                // them. Keep them out of `Resources/` and copy verbatim.
                .copy("ShellIntegrationResources")
            ],
            plugins: [
                .plugin(name: "SwiftLintBuildToolPlugin", package: "SwiftLint")
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
