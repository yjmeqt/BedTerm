// swift-tools-version:6.2
import PackageDescription

let package = Package(
    name: "BedTermKit",
    platforms: [.iOS(.v26)],
    products: [
        .library(name: "BedTermKit", targets: ["BedTermKit"])
    ],
    dependencies: [
        .package(url: "https://github.com/migueldeicaza/SwiftTerm", from: "1.2.0"),
        .package(url: "https://github.com/orlandos-nl/Citadel", from: "0.7.0"),
        .package(url: "https://github.com/realm/SwiftLint", from: "0.57.0")
    ],
    targets: [
        .binaryTarget(
            name: "BedTermCore",
            path: "BinaryFrameworks/BedTermCore.xcframework"
        ),
        .target(
            name: "BedTermKit",
            dependencies: [
                "BedTermCore",
                .product(name: "SwiftTerm", package: "SwiftTerm"),
                .product(name: "Citadel", package: "Citadel")
            ],
            path: "Sources/BedTermKit",
            resources: [
                .process("Resources")
            ],
            plugins: [
                .plugin(name: "SwiftLintBuildToolPlugin", package: "SwiftLint")
            ]
        )
    ],
    swiftLanguageModes: [.v5]
)
