// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "SwiftPreviewHost",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(name: "SwiftPreviewHost", targets: ["SwiftPreviewHost"]),
        .executable(name: "swift-preview-host-client", targets: ["SwiftPreviewHostClient"]),
    ],
    targets: [
        .target(name: "SwiftPreviewHost"),
        .executableTarget(
            name: "SwiftPreviewHostClient",
            dependencies: ["SwiftPreviewHost"]
        ),
        .testTarget(name: "SwiftPreviewHostTests", dependencies: ["SwiftPreviewHost"]),
    ]
)
