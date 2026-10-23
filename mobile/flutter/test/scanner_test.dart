import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:rcode_flutter/rcode_flutter.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const methods = MethodChannel('rcode_flutter/methods');
  const codec = StandardMethodCodec();
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  final calls = <String>[];

  setUp(() {
    calls.clear();
    messenger.setMockMethodCallHandler(methods, (call) async {
      calls.add(call.method);
      if (call.method == 'decodeImage') {
        return <String, Object>{
          'payload': <int>[72, 105],
          'correctedBytes': 0,
          'landmarks': <List<double>>[
            <double>[1, 2],
            <double>[3, 4],
            <double>[5, 6],
            <double>[7, 8],
          ],
          'imageWidth': 20,
          'imageHeight': 20,
          'spokes': 54,
          'errorCorrection': 2,
        };
      }
      if (call.method == 'permissionState' ||
          call.method == 'requestPermission') {
        return 1;
      }
      return null;
    });
  });

  tearDown(() async {
    messenger.setMockMethodCallHandler(methods, null);
  });

  test('reports denied camera permission', () async {
    final controller = RCodeScannerController();
    await controller.attachForTesting(9);
    expect(await controller.permissionState, CameraPermissionState.required);
    expect(
      await controller.requestPermission(),
      CameraPermissionState.required,
    );
    await controller.dispose();
  });

  test('decodes one encoded image independently of camera preview', () async {
    final scanner = RCodeScannerController();
    final result = await scanner.decodeImage(
      Uint8List.fromList(<int>[1, 2, 3]),
    );
    expect(result.payload, <int>[72, 105]);
    expect(calls, <String>['decodeImage']);
    await scanner.dispose();
  });

  test('start and stop are forwarded across lifecycle pauses', () async {
    final controller = RCodeScannerController();
    await controller.attachForTesting(10);
    await controller.start();
    await controller.stop();
    await controller.start();
    expect(calls, containsAllInOrder(<String>['start', 'stop', 'start']));
    await controller.dispose();
  });

  test('emits only one result per start', () async {
    final controller = RCodeScannerController();
    final results = <ScanResult>[];
    final subscription = controller.results.listen(results.add);
    await controller.attachForTesting(11);
    await Future<void>.delayed(Duration.zero);
    const event = <String, Object>{
      'type': 'result',
      'payload': <int>[65, 66],
      'correctedBytes': 0,
      'landmarks': <List<double>>[
        <double>[1, 2],
        <double>[3, 4],
        <double>[5, 6],
        <double>[7, 8],
      ],
      'imageWidth': 100,
      'imageHeight': 100,
      'spokes': 54,
      'errorCorrection': 2,
    };
    final data = codec.encodeSuccessEnvelope(event);
    messenger.handlePlatformMessage('rcode_flutter/events/11', data, (_) {});
    messenger.handlePlatformMessage('rcode_flutter/events/11', data, (_) {});
    await Future<void>.delayed(Duration.zero);
    expect(results, hasLength(1));
    expect(results.single.payload, Uint8List.fromList(<int>[65, 66]));
    await subscription.cancel();
    await controller.dispose();
  });
}
