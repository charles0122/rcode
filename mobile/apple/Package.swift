// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "RCodeApple",
    platforms: [.iOS(.v15)],
    products: [
        .library(name: "RCodeApple", targets: ["RCodeApple"]),
    ],
    targets: [
        .binaryTarget(name: "RCodeCore", path: "../../dist/core/apple/RCode.xcframework"),
        .target(name: "RCodeApple", dependencies: ["RCodeCore"]),
        .testTarget(
            name: "RCodeAppleTests",
            dependencies: ["RCodeApple"],
            resources: [.copy("Fixtures")]
        ),
    ]
)
