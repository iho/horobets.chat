// swift-tools-version: 6.1
// This is a Skip (https://skip.dev) package.
import PackageDescription

let package = Package(
    name: "mobile",
    defaultLocalization: "en",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "MessengerApp", type: .dynamic, targets: ["MessengerApp"]),
    ],
    dependencies: [
        .package(url: "https://github.com/skiptools/skip.git", exact: "1.9.21"),
        .package(url: "https://github.com/skiptools/skip-ui.git", from: "1.0.0"),
        .package(path: "Bridge")
    ],
    targets: [
        .target(name: "MessengerApp", dependencies: [
            .product(name: "SkipUI", package: "skip-ui"),
            .product(name: "MessengerCore", package: "Bridge")
        ], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .testTarget(name: "MessengerAppTests", dependencies: ["MessengerApp", .product(name: "MessengerCore", package: "Bridge")]),
    ]
)
