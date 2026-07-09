// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "RockboxFFI",
    products: [
        .library(name: "RockboxFFI", targets: ["RockboxFFI"]),
        .executable(name: "rockbox-ffi-smoke", targets: ["rockbox-ffi-smoke"]),
        .executable(name: "rockbox-ffi-play", targets: ["rockbox-ffi-play"]),
    ],
    targets: [
        // Pure-Swift binding: dlopen's the prebuilt librockbox_ffi at runtime.
        // No C target / module map / linker flags — the shared library is
        // located and loaded dynamically, exactly like the Ruby/Python bindings.
        .target(name: "RockboxFFI"),
        .executableTarget(
            name: "rockbox-ffi-smoke",
            dependencies: ["RockboxFFI"]
        ),
        .executableTarget(
            name: "rockbox-ffi-play",
            dependencies: ["RockboxFFI"]
        ),
    ]
)
