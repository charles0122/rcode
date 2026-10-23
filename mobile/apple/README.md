# RCodeApple

An iOS Swift Package wrapper around the Rust decoder's C ABI. It uses the
prebuilt `dist/core/apple/RCode.xcframework`; the package does not implement
the RCode format or decoding algorithm.

## Build and tests

Run the tests on an installed iOS simulator runtime:

```sh
cd mobile/apple
xcodebuild -scheme RCodeApple -destination 'platform=iOS Simulator,name=iPhone 18 Pro' test
```

The package supports iOS 15 and later and currently includes an arm64 device
slice and an arm64 simulator slice. To regenerate the Apple core artifact,
run `scripts/build-mobile-core.sh` from the repository root.

Add `NSCameraUsageDescription` to the host app's Info.plist before requesting
camera access.

## Camera scanning

```swift
let scanner = RCodeScanner()
previewContainer.layer.addSublayer(scanner.previewLayer)
scanner.previewLayer.frame = previewContainer.bounds
scanner.onResult = { result in
    print(String(decoding: result.payload, as: UTF8.self))
}
scanner.onError = { error in
    print(error.localizedDescription)
}
scanner.start()
```

Call `stop()` when leaving the scanning screen. The scanner requests the rear
camera, consumes the bi-planar Y plane with its actual `bytesPerRow`, discards
late frames, and emits at most one successful result per start. Backgrounding
stops capture; returning to foreground resumes it. `previewPoint(for:in:)`
uses `AVCaptureVideoPreviewLayer` conversion, which accounts for the preview
orientation and aspect-fill crop. The rear camera preview is never mirrored.
The default orientation is portrait; call `setVideoOrientation(_:)` when the
host interface rotates so the preview and analyzed frames stay aligned.

Use `CameraPermission.status` to inspect authorization, or
`CameraPermission.request` to request it separately. `setTorchEnabled(_:)`
controls the rear torch; it throws when the torch is unavailable.

For a single gallery image, call `try LumaDecoder().decode(imageData:)` with
PNG/JPEG data, or `decode(image:)` with a `UIImage`. UIKit applies image
orientation and converts the image to one packed luma plane before invoking the
same Rust decoder used by live capture.

The automated tests cover the public result models, one-result gate, lifecycle
intent, preview-layer coordinate conversion, invalid luma input, and decoding a
current v3 encoded-image fixture. They do not verify live camera capture or
physical-device scanning.
