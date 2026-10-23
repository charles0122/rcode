# RCode Flutter plugin

This plugin provides a native rear-camera preview, permission control, torch control, and one
`ScanResult` event per start. Camera frames stay in CameraX/AVFoundation; only control calls and
the decoded result cross Flutter platform channels. The plugin requests camera permission when
asked; the host app should call `stop()` when the scanner screen is no longer active. Native SDK lifecycle handling
pauses and resumes an active scan with the app lifecycle.

```dart
final scanner = RCodeScannerController();
// Place RCodeScannerView(controller: scanner) in the widget tree.
final permission = await scanner.requestPermission();
if (permission == CameraPermissionState.granted) await scanner.start();
final imageResult = await scanner.decodeImage(encodedImageBytes); // independent single-image API
// Listen to scanner.results and scanner.errors; call scanner.dispose() when finished.
```

The example app demonstrates the preview, overlay, permission request, torch, and result UI.
`decodeImage(Uint8List)` decodes one PNG/JPEG on the native platform and returns the same
`ScanResult` model without opening the camera. Only this explicit one-image call sends image bytes
over a method call; continuous camera frames never cross a platform channel.

## Monorepo development integration

The example's Android settings include `../../../../mobile/android/rcode-scanner` as a Gradle
project. Its iOS podspec compiles the existing `mobile/apple/Sources/RCodeApple` wrapper and links
`dist/core/apple/RCode.xcframework`. Build the core artifacts first from the repository root with
`scripts/build-mobile-core.sh`.

These relative paths are for this repository checkout and are not publishable dependencies.
Before publishing the Flutter package, replace them with versioned AAR and XCFramework/Swift
package artifacts and pin their shared RCode release version. Declare `CAMERA` in Android host
manifests and `NSCameraUsageDescription` in iOS host Info.plists.

## Verification

From this directory, run `flutter analyze` and `flutter test`. The example builds can be checked
with `cd example && flutter build apk --debug` and
`cd example && flutter build ios --simulator --no-codesign`. Builds do not establish camera or
physical-device scan acceptance.
