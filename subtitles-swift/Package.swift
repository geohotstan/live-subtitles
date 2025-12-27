// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "subtitles-swift",
    platforms: [
        .macOS(.v15)
    ],
    products: [
        .executable(name: "subtitles-swift", targets: ["SubtitlesApp"])
    ],
    targets: [
        .executableTarget(
            name: "SubtitlesApp",
            path: "Sources",
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("Speech"),
                .linkedFramework("SwiftUI"),
                .linkedFramework("Translation")
            ]
        )
    ]
)
