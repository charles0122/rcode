import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:rcode_flutter/rcode_flutter.dart';

void main() => runApp(const RCodeExample());

class RCodeExample extends StatelessWidget {
  const RCodeExample({super.key});

  @override
  Widget build(BuildContext context) => MaterialApp(
    theme: ThemeData(colorSchemeSeed: const Color(0xFF173B32)),
    home: DefaultTabController(
      length: 2,
      child: Scaffold(
        appBar: AppBar(
          title: Text('RCode'),
          bottom: TabBar(
            tabs: <Widget>[
              Tab(text: '生成'),
              Tab(text: '扫描'),
            ],
          ),
        ),
        body: const TabBarView(
          children: <Widget>[GeneratorPage(), ScannerPage()],
        ),
      ),
    ),
  );
}

class GeneratorPage extends StatefulWidget {
  const GeneratorPage({super.key});

  @override
  State<GeneratorPage> createState() => _GeneratorPageState();
}

class _GeneratorPageState extends State<GeneratorPage> {
  final _encoder = RCodeEncoder();
  final _textController = TextEditingController(text: 'Hello RCode');
  RCodeStyle _style = RCodeStyle.rings;
  RenderPlan? _plan;
  String? _error;
  bool _encoding = false;

  @override
  void dispose() {
    _textController.dispose();
    super.dispose();
  }

  Future<void> _generate() async {
    setState(() {
      _encoding = true;
      _error = null;
    });
    try {
      final plan = await _encoder.encode(
        Uint8List.fromList(utf8.encode(_textController.text)),
        style: _style,
      );
      if (mounted) setState(() => _plan = plan);
    } on Object catch (error) {
      if (mounted) setState(() => _error = '$error');
    } finally {
      if (mounted) setState(() => _encoding = false);
    }
  }

  @override
  Widget build(BuildContext context) => ListView(
    padding: const EdgeInsets.all(16),
    children: <Widget>[
      TextField(
        controller: _textController,
        decoration: const InputDecoration(
          border: OutlineInputBorder(),
          labelText: '编码内容',
        ),
        minLines: 2,
        maxLines: 4,
      ),
      const SizedBox(height: 12),
      SegmentedButton<RCodeStyle>(
        segments: const <ButtonSegment<RCodeStyle>>[
          ButtonSegment(value: RCodeStyle.lines, label: Text('放射线')),
          ButtonSegment(value: RCodeStyle.dots, label: Text('圆点')),
          ButtonSegment(value: RCodeStyle.rings, label: Text('断点圆环')),
        ],
        selected: <RCodeStyle>{_style},
        onSelectionChanged: (styles) => setState(() => _style = styles.single),
      ),
      const SizedBox(height: 12),
      FilledButton(
        onPressed: _encoding ? null : _generate,
        child: Text(_encoding ? '生成中…' : '生成'),
      ),
      if (_error != null) ...<Widget>[
        const SizedBox(height: 12),
        Text(
          _error!,
          style: TextStyle(color: Theme.of(context).colorScheme.error),
        ),
      ],
      if (_plan != null) ...<Widget>[
        const SizedBox(height: 20),
        AspectRatio(
          aspectRatio: 1,
          child: DecoratedBox(
            decoration: BoxDecoration(
              border: Border.all(
                color: Theme.of(context).colorScheme.outlineVariant,
              ),
            ),
            child: RCodeView(plan: _plan!),
          ),
        ),
        const SizedBox(height: 8),
        const Text('可用另一台设备打开“扫描”页识别此码。', textAlign: TextAlign.center),
      ],
    ],
  );
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
      setState(() => _message = 'Point the camera at an RCode.');
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
