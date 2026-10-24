import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

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

enum RCodeStyle { lines, dots, rings }

enum DrawKind { circle, line, arc }

enum DrawPaint { background, foreground }

@immutable
class DrawCommand {
  const DrawCommand({
    required this.kind,
    required this.paint,
    required this.values,
  });
  final DrawKind kind;
  final DrawPaint paint;
  final List<double> values;
}

@immutable
class RenderPlan {
  const RenderPlan({
    required this.version,
    required this.viewport,
    required this.logoRegion,
    required this.commands,
  });

  final int version;
  final double viewport;
  final List<double> logoRegion;
  final List<DrawCommand> commands;

  factory RenderPlan.fromMap(Map<Object?, Object?> map) {
    final logo = (map['logoRegion']! as List<Object?>).cast<num>();
    final rawCommands = map['commands']! as List<Object?>;
    final version = map['version']! as int;
    if (version != 1) {
      throw const FormatException('Unsupported RenderPlan version.');
    }
    return RenderPlan(
      version: version,
      viewport: (map['viewport']! as num).toDouble(),
      logoRegion: List.unmodifiable(logo.map((value) => value.toDouble())),
      commands: List.unmodifiable(
        rawCommands.map((raw) {
          final command = raw! as Map<Object?, Object?>;
          return DrawCommand(
            kind: DrawKind.values[command['kind']! as int],
            paint: DrawPaint.values[command['paint']! as int],
            values: List.unmodifiable(
              (command['values']! as List<Object?>).cast<num>().map(
                (value) => value.toDouble(),
              ),
            ),
          );
        }),
      ),
    );
  }
}

class RCodeEncoder {
  static const _methods = MethodChannel('rcode_flutter/methods');

  Future<RenderPlan> encode(
    Uint8List payload, {
    ScanProfile profile = const ScanProfile(
      spokes: 72,
      errorCorrection: ErrorCorrection.medium,
    ),
    RCodeStyle style = RCodeStyle.lines,
  }) async {
    final map = await _methods
        .invokeMapMethod<Object?, Object?>('encodePlan', <String, Object?>{
          'payload': payload,
          'spokes': profile.spokes,
          'errorCorrection': profile.errorCorrection.index,
          'style': style.index,
        });
    if (map == null) throw PlatformException(code: 'internal');
    return RenderPlan.fromMap(map);
  }
}

class RCodeView extends StatelessWidget {
  const RCodeView({
    super.key,
    required this.plan,
    this.foreground = const Color(0xFF173B32),
    this.background = const Color(0xFFFFFFFF),
    this.logo,
  });

  final RenderPlan plan;
  final Color foreground;
  final Color background;
  final ui.Image? logo;

  @override
  Widget build(BuildContext context) => CustomPaint(
    painter: _RCodePainter(plan, foreground, background, logo),
    size: Size.infinite,
  );
}

class _RCodePainter extends CustomPainter {
  const _RCodePainter(this.plan, this.foreground, this.background, this.logo);
  final RenderPlan plan;
  final Color foreground;
  final Color background;
  final ui.Image? logo;

  @override
  void paint(Canvas canvas, Size size) {
    final side = math.min(size.width, size.height);
    final origin = Offset((size.width - side) / 2, (size.height - side) / 2);
    final scale = side / plan.viewport;
    canvas.drawRect(Offset.zero & size, Paint()..color = background);
    canvas.save();
    canvas.translate(origin.dx, origin.dy);
    canvas.scale(scale);
    for (final command in plan.commands) {
      final values = command.values;
      final color = command.paint == DrawPaint.foreground
          ? foreground
          : background;
      final brush = Paint()..color = color;
      switch (command.kind) {
        case DrawKind.circle:
          canvas.drawCircle(Offset(values[0], values[1]), values[2], brush);
        case DrawKind.line:
          if (values[0] == values[2] && values[1] == values[3]) {
            canvas.drawCircle(
              Offset(values[0], values[1]),
              values[4] / 2,
              brush,
            );
          } else {
            brush
              ..style = PaintingStyle.stroke
              ..strokeWidth = values[4]
              ..strokeCap = StrokeCap.round;
            canvas.drawLine(
              Offset(values[0], values[1]),
              Offset(values[2], values[3]),
              brush,
            );
          }
        case DrawKind.arc:
          brush
            ..style = PaintingStyle.stroke
            ..strokeWidth = values[5]
            ..strokeCap = StrokeCap.round;
          canvas.drawArc(
            Rect.fromCircle(
              center: Offset(values[0], values[1]),
              radius: values[2],
            ),
            values[3],
            values[4],
            false,
            brush,
          );
      }
    }
    if (logo != null && plan.logoRegion.length == 3) {
      final cx = plan.logoRegion[0];
      final cy = plan.logoRegion[1];
      final radius = plan.logoRegion[2];
      final target = Rect.fromCircle(center: Offset(cx, cy), radius: radius);
      canvas.save();
      canvas.clipPath(Path()..addOval(target));
      canvas.drawImageRect(
        logo!,
        Rect.fromLTWH(0, 0, logo!.width.toDouble(), logo!.height.toDouble()),
        target,
        Paint(),
      );
      canvas.restore();
    }
    canvas.restore();
  }

  @override
  bool shouldRepaint(covariant _RCodePainter oldDelegate) =>
      oldDelegate.plan != plan ||
      oldDelegate.foreground != foreground ||
      oldDelegate.background != background ||
      oldDelegate.logo != logo;
}

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
  final _previewTextureId = ValueNotifier<int?>(null);

  Stream<ScanResult> get results => _results.stream;
  Stream<ScanError> get errors => _errors.stream;

  Future<void> _attach(int viewId, {int? textureId}) async {
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
    if (defaultTargetPlatform == TargetPlatform.iOS) {
      _previewTextureId.value = textureId ?? await _call<int>('textureId');
    }
  }

  Future<void> _attachIos() async {
    final response = await _methods.invokeMapMethod<Object?, Object?>(
      'createScanner',
    );
    if (response == null) throw PlatformException(code: 'internal');
    await _attach(
      response['viewId']! as int,
      textureId: response['textureId']! as int,
    );
  }

  Future<void> _detachIos() async {
    final id = _viewId;
    if (id == null) return;
    _viewId = null;
    _previewTextureId.value = null;
    await _subscription?.cancel();
    _subscription = null;
    await _methods.invokeMethod<void>('disposeScanner', <String, Object?>{
      'viewId': id,
    });
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
    if (defaultTargetPlatform == TargetPlatform.iOS) {
      await _detachIos();
    } else {
      await stopIfAttached();
    }
    await _subscription?.cancel();
    await _results.close();
    await _errors.close();
    _previewTextureId.dispose();
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

class _NativePreview extends StatefulWidget {
  const _NativePreview(this.controller);

  final RCodeScannerController controller;

  @override
  State<_NativePreview> createState() => _NativePreviewState();
}

class _NativePreviewState extends State<_NativePreview> {
  @override
  void initState() {
    super.initState();
    if (defaultTargetPlatform == TargetPlatform.iOS) {
      unawaited(widget.controller._attachIos());
    }
  }

  @override
  void dispose() {
    if (defaultTargetPlatform == TargetPlatform.iOS) {
      unawaited(widget.controller._detachIos());
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final creationParams = <String, Object?>{};
    if (defaultTargetPlatform == TargetPlatform.android) {
      return AndroidView(
        viewType: 'rcode_flutter/preview',
        creationParams: creationParams,
        creationParamsCodec: const StandardMessageCodec(),
        onPlatformViewCreated: widget.controller._attach,
      );
    }
    if (defaultTargetPlatform == TargetPlatform.iOS) {
      return Stack(
        fit: StackFit.expand,
        children: <Widget>[
          const ColoredBox(color: Color(0xFF101010)),
          ValueListenableBuilder<int?>(
            valueListenable: widget.controller._previewTextureId,
            builder: (context, textureId, child) => textureId == null
                ? const SizedBox.shrink()
                : Texture(textureId: textureId),
          ),
        ],
      );
    }
    return const Center(
      child: Text('RCode camera preview is available on iOS and Android.'),
    );
  }
}
