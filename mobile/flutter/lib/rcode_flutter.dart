import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

enum CameraPermissionState { granted, required }

enum ScanError {
  permissionRequired,
  permissionDenied,
  cameraUnavailable,
  noCode,
  invalidArgument,
  corruptCode,
  invalidImage,
  internal,
}

enum ErrorCorrection { low, medium, quartile, high }

@immutable
class Landmark {
  const Landmark(this.x, this.y);
  final double x;
  final double y;
}

@immutable
class ScanProfile {
  const ScanProfile({required this.spokes, required this.errorCorrection});
  final int spokes;
  final ErrorCorrection errorCorrection;
}

@immutable
class ScanResult {
  const ScanResult({
    required this.payload,
    required this.correctedBytes,
    required this.landmarks,
    required this.imageWidth,
    required this.imageHeight,
    required this.profile,
  });

  final Uint8List payload;
  final int correctedBytes;
  final List<Landmark> landmarks;
  final int imageWidth;
  final int imageHeight;
  final ScanProfile profile;

  factory ScanResult.fromMap(Map<Object?, Object?> map) {
    final rawPoints = map['landmarks']! as List<Object?>;
    return ScanResult(
      payload: Uint8List.fromList(
        map['payload']! is Uint8List
            ? map['payload']! as Uint8List
            : (map['payload']! as List<Object?>).cast<int>(),
      ),
      correctedBytes: map['correctedBytes']! as int,
      landmarks: List.unmodifiable(
        rawPoints.map((point) {
          final values = (point! as List<Object?>).cast<num>();
          return Landmark(values[0].toDouble(), values[1].toDouble());
        }),
      ),
      imageWidth: map['imageWidth']! as int,
      imageHeight: map['imageHeight']! as int,
      profile: ScanProfile(
        spokes: map['spokes']! as int,
        errorCorrection: ErrorCorrection.values[map['errorCorrection']! as int],
      ),
    );
  }
}

class RCodeScannerController {
  static const _methods = MethodChannel('rcode_flutter/methods');
  final _results = StreamController<ScanResult>.broadcast();
  final _errors = StreamController<ScanError>.broadcast();
  EventChannel? _events;
  StreamSubscription<Object?>? _subscription;
  int? _viewId;
  bool _delivered = false;

  Stream<ScanResult> get results => _results.stream;
  Stream<ScanError> get errors => _errors.stream;

  Future<void> _attach(int viewId) async {
    _viewId = viewId;
    _events = EventChannel('rcode_flutter/events/$viewId');
    await _subscription?.cancel();
    _subscription = _events!.receiveBroadcastStream().listen(
      (event) {
        final message = event! as Map<Object?, Object?>;
        if (message['type'] == 'result') {
          if (_delivered) return;
          _delivered = true;
          _results.add(ScanResult.fromMap(message));
        } else if (message['type'] == 'error') {
          final code = message['code']! as String;
          _errors.add(ScanError.values.byName(code));
        }
      },
      onError: (Object error) {
        _errors.add(ScanError.internal);
      },
    );
  }

  @visibleForTesting
  Future<void> attachForTesting(int viewId) => _attach(viewId);

  Future<CameraPermissionState> get permissionState async =>
      CameraPermissionState.values[await _call<int>('permissionState')];

  Future<CameraPermissionState> requestPermission() async =>
      CameraPermissionState.values[await _call<int>('requestPermission')];

  Future<void> start() async {
    _delivered = false;
    await _call<void>('start');
  }

  Future<void> stop() => _call<void>('stop');
  Future<bool> setTorch(bool enabled) async =>
      (await _call<int>('setTorch', enabled)) == 1;

  /// Decodes one encoded image without starting or using the camera preview.
  Future<ScanResult> decodeImage(Uint8List encodedImage) async {
    final map = await _methods.invokeMapMethod<Object?, Object?>(
      'decodeImage',
      encodedImage,
    );
    if (map == null) throw PlatformException(code: 'internal');
    return ScanResult.fromMap(map);
  }

  Future<T> _call<T>(String method, [Object? value]) async {
    final id = _viewId;
    if (id == null) throw StateError('Scanner preview is not mounted.');
    return (await _methods.invokeMethod<T>(method, <String, Object?>{
          'viewId': id,
          'enabled': value,
        }))
        as T;
  }

  Future<void> dispose() async {
    await stopIfAttached();
    await _subscription?.cancel();
    await _results.close();
    await _errors.close();
    _viewId = null;
  }

  Future<void> stopIfAttached() async {
    if (_viewId != null) await stop();
  }
}

class RCodeScannerView extends StatelessWidget {
  const RCodeScannerView({super.key, required this.controller, this.overlay});

  final RCodeScannerController controller;
  final Widget? overlay;

  @override
  Widget build(BuildContext context) => Stack(
    fit: StackFit.expand,
    children: <Widget>[_NativePreview(controller), ?overlay],
  );
}

class _NativePreview extends StatelessWidget {
  const _NativePreview(this.controller);

  final RCodeScannerController controller;

  @override
  Widget build(BuildContext context) {
    final creationParams = <String, Object?>{};
    if (defaultTargetPlatform == TargetPlatform.android) {
      return AndroidView(
        viewType: 'rcode_flutter/preview',
        creationParams: creationParams,
        creationParamsCodec: const StandardMessageCodec(),
        onPlatformViewCreated: controller._attach,
      );
    }
    if (defaultTargetPlatform == TargetPlatform.iOS) {
      return UiKitView(
        viewType: 'rcode_flutter/preview',
        creationParams: creationParams,
        creationParamsCodec: const StandardMessageCodec(),
        onPlatformViewCreated: controller._attach,
      );
    }
    return const Center(
      child: Text('RCode camera preview is available on iOS and Android.'),
    );
  }
}
