import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:rcode_flutter/rcode_flutter.dart';

void main() => runApp(const RCodeExample());

class RCodeExample extends StatelessWidget {
  const RCodeExample({super.key});

  @override
  Widget build(BuildContext context) => const MaterialApp(home: ScannerPage());
}

class ScannerPage extends StatefulWidget {
  const ScannerPage({super.key});

  @override
  State<ScannerPage> createState() => _ScannerPageState();
}

class _ScannerPageState extends State<ScannerPage> {
  final _scanner = RCodeScannerController();
  StreamSubscription<ScanResult>? _resultSubscription;
  StreamSubscription<ScanError>? _errorSubscription;
  String _message = 'Camera permission is required to scan.';

  @override
  void initState() {
    super.initState();
    _resultSubscription = _scanner.results.listen((result) {
      setState(() => _message = String.fromCharCodes(result.payload));
    });
    _errorSubscription = _scanner.errors.listen((error) {
      setState(() => _message = 'Scanner error: ${error.name}');
    });
  }

  @override
  void dispose() {
    unawaited(_resultSubscription?.cancel());
    unawaited(_errorSubscription?.cancel());
    unawaited(_scanner.dispose());
    super.dispose();
  }

  Future<void> _start() async {
    final permission = await _scanner.requestPermission();
    if (!mounted) return;
    if (permission == CameraPermissionState.granted) {
      await _scanner.start();
    } else {
      setState(() => _message = 'Camera permission was denied.');
    }
  }

  Future<void> _decodeSample() async {
    final sample = await rootBundle.load('assets/current-v3.png');
    final bytes = Uint8List.view(
      sample.buffer,
      sample.offsetInBytes,
      sample.lengthInBytes,
    );
    final result = await _scanner.decodeImage(bytes);
    if (mounted) {
      setState(() => _message = String.fromCharCodes(result.payload));
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('RCode scanner')),
    body: Column(
      children: <Widget>[
        Expanded(
          child: RCodeScannerView(
            controller: _scanner,
            overlay: Center(
              child: Container(
                width: 240,
                height: 240,
                decoration: BoxDecoration(
                  border: Border.all(color: Colors.white, width: 2),
                ),
              ),
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Text(_message, textAlign: TextAlign.center),
        ),
        Wrap(
          spacing: 12,
          children: <Widget>[
            FilledButton(onPressed: _start, child: const Text('Start scan')),
            OutlinedButton(
              onPressed: _decodeSample,
              child: const Text('Decode sample image'),
            ),
            OutlinedButton(onPressed: _scanner.stop, child: const Text('Stop')),
            OutlinedButton(
              onPressed: () async => _scanner.setTorch(true),
              child: const Text('Torch on'),
            ),
            OutlinedButton(
              onPressed: () async => _scanner.setTorch(false),
              child: const Text('Torch off'),
            ),
          ],
        ),
        const SizedBox(height: 16),
      ],
    ),
  );
}
