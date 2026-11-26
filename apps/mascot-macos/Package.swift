// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "NexusMascot",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "NexusMascot", targets: ["NexusMascot"])
    ],
    targets: [
        .executableTarget(
            name: "NexusMascot",
            path: "Sources/NexusMascot",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("ApplicationServices"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("Carbon"),
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("Speech")
            ]
        )
    ]
)
