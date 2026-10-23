# Android scanner library

`rcode-scanner` is an Android library for scanning one RCode at a time. It uses
CameraX `Preview` and `ImageAnalysis` (`KEEP_ONLY_LATEST`), passes the Y plane's
direct buffer, width, height, and row stride through JNI to the stable C ABI,
and lets the Rust core perform decoding. The Rust decoder makes its documented
single luma packing copy; the Android layer does not copy the camera plane.

The library targets minSdk 24 and compileSdk 36, and packages `arm64-v8a` and
`x86_64`. Build Rust mobile libraries once from the repository root with
`scripts/build-mobile-core.sh`, then build the Android AAR:

```sh
cd mobile/android
./gradlew :rcode-scanner:assembleRelease
```

The AAR is written to `rcode-scanner/build/outputs/aar/`. Run JVM tests and lint
with `./gradlew :rcode-scanner:testDebugUnitTest :rcode-scanner:lint`.

## Usage

Add a `PreviewView` to the host screen, declare the camera permission in the app,
and request it using the host's normal Android permission flow. The scanner
reports `CameraPermissionState.REQUIRED` and never requests permission itself.

```kotlin
val scanner = RCodeScanner(
    context = requireContext(),
    lifecycleOwner = viewLifecycleOwner,
    previewView = binding.preview,
    onResult = { result ->
        val text = result.payload.toString(Charsets.UTF_8)
        // result.landmarks are in PreviewView coordinates.
    },
    onError = { error -> /* handle permission or camera errors */ },
)

if (scanner.permissionState() == CameraPermissionState.GRANTED) {
    scanner.start()
} else {
    // Request Manifest.permission.CAMERA, then call start() after grant.
}
// scanner.setTorch(true) / scanner.setTorch(false)
// scanner.stop() when scanning is paused; scanner.close() when the screen is destroyed.
```

`start()` binds when the lifecycle is foregrounded and permission is granted;
`stop()` unbinds, and lifecycle stop/start pauses and resumes an active request.
Each `start()` delivers at most one result until stopped and started again. The
first release intentionally supports only the rear camera. CameraX maps the
original analysis-buffer landmarks into `PreviewView` coordinates, including
its rotation and crop. Camera permissions, UI, and camera lifecycle decisions
remain owned by the host app.

For a single gallery image, call `RCodeDecoder().decodeImage(encodedBytes)` on
a worker thread. PNG/JPEG decoding and EXIF orientation normalization happen on
Android, then the packed luma plane is passed through the same native decoder.
`decodeLuma` is also public for callers that already own an 8-bit direct buffer.
