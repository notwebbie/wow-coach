// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "WoWCoach",
    platforms: [.macOS(.v13)],
    products: [.executable(name: "WoWCoach", targets: ["WoWCoach"])],
    targets: [
        .executableTarget(name: "WoWCoach"),
        .testTarget(name: "WoWCoachTests", dependencies: ["WoWCoach"])
    ]
)
