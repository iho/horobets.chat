// swift-tools-version: 6.1
import PackageDescription
let package = Package(
    name: "MessengerCore",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [.library(name: "MessengerCore", targets: ["MessengerCore"])],
    targets: [
        .binaryTarget(name: "MessengerCoreFFI", path: "MessengerCoreFFI.xcframework"),
        .target(name: "MessengerCore", dependencies: ["MessengerCoreFFI"], linkerSettings: [
            .linkedLibrary("resolv"), .linkedLibrary("iconv"), .linkedLibrary("c++"),
            .linkedFramework("Security"), .linkedFramework("SystemConfiguration")
        ])
    ]
)
