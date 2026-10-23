import AVFoundation
import Flutter
import RCodeApple
import UIKit

public class RcodeFlutterPlugin: NSObject, FlutterPlugin {
  private let registrar: FlutterPluginRegistrar
  private var views: [Int64: ScannerView] = [:]

  init(registrar: FlutterPluginRegistrar) { self.registrar = registrar }

  public static func register(with registrar: FlutterPluginRegistrar) {
    let instance = RcodeFlutterPlugin(registrar: registrar)
    registrar.addMethodCallDelegate(instance, channel: FlutterMethodChannel(name: "rcode_flutter/methods", binaryMessenger: registrar.messenger()))
    registrar.register(PreviewFactory(plugin: instance), withId: "rcode_flutter/preview")
  }

  public func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    let args = call.arguments as? [String: Any] ?? [:]
    let view = (args["viewId"] as? NSNumber).flatMap { views[$0.int64Value] }
    switch call.method {
    case "decodeImage":
      guard let image = call.arguments as? FlutterStandardTypedData else {
        result(FlutterError(code: "invalidImage", message: "Expected encoded image bytes", details: nil)); return
      }
      do { result(Self.resultMap(try LumaDecoder().decode(imageData: image.data))) }
      catch let error as ScanError { result(FlutterError(code: Self.errorCode(error), message: error.localizedDescription, details: nil)) }
      catch { result(FlutterError(code: "internal", message: error.localizedDescription, details: nil)) }
    case "permissionState": result(AVCaptureDevice.authorizationStatus(for: .video) == .authorized ? 0 : 1)
    case "requestPermission":
      CameraPermission.request { result($0 ? 0 : 1) }
    case "start":
      guard let view else { result(FlutterError(code: "not_ready", message: "Preview is not ready", details: nil)); return }
      view.start()
      result(nil)
    case "stop": view?.stop(); result(nil)
    case "setTorch":
      do { try view?.setTorch((args["enabled"] as? Bool) ?? false); result(view == nil ? 0 : 1) }
      catch { result(0) }
    default: result(FlutterMethodNotImplemented)
    }
  }

  private static func resultMap(_ result: ScanResult) -> [String: Any] {
    ["payload": FlutterStandardTypedData(bytes: result.payload),
     "correctedBytes": result.correctedBytes,
     "landmarks": result.landmarks.map { [$0.x, $0.y] },
     "imageWidth": result.imageWidth, "imageHeight": result.imageHeight,
     "spokes": result.profile.spokes, "errorCorrection": Int(result.profile.errorCorrection.rawValue)]
  }

  private static func errorCode(_ error: ScanError) -> String {
    switch error {
    case .permissionDenied: "permissionDenied"
    case .cameraUnavailable: "cameraUnavailable"
    case .noCode: "noCode"
    case .invalidArgument: "invalidArgument"
    case .corruptCode: "corruptCode"
    case .invalidImage: "invalidImage"
    default: "internal"
    }
  }

  private final class PreviewFactory: NSObject, FlutterPlatformViewFactory {
    weak var plugin: RcodeFlutterPlugin?
    init(plugin: RcodeFlutterPlugin) { self.plugin = plugin }
    func createArgsCodec() -> (FlutterMessageCodec & NSObjectProtocol) { FlutterStandardMessageCodec.sharedInstance() }
    func create(withFrame frame: CGRect, viewIdentifier viewId: Int64, arguments args: Any?) -> FlutterPlatformView {
      let view = ScannerView(frame: frame, id: viewId, messenger: plugin!.registrar.messenger()) { [weak plugin] id in
        plugin?.views.removeValue(forKey: id)
      }
      plugin?.views[viewId] = view
      return view
    }
  }

  private final class ScannerView: NSObject, FlutterPlatformView, FlutterStreamHandler {
    private let root: PreviewContainerView
    private let scanner = RCodeScanner()
    private let eventChannel: FlutterEventChannel
    private var sink: FlutterEventSink?
    private let id: Int64

    init(frame: CGRect, id: Int64, messenger: FlutterBinaryMessenger, onDispose: @escaping (Int64) -> Void) {
      self.id = id
      root = PreviewContainerView(frame: frame)
      eventChannel = FlutterEventChannel(name: "rcode_flutter/events/\(id)", binaryMessenger: messenger)
      super.init()
      eventChannel.setStreamHandler(self)
      scanner.previewLayer.videoGravity = .resizeAspectFill
      scanner.previewLayer.frame = root.bounds
      root.layer.addSublayer(scanner.previewLayer)
      root.onLayout = { [weak scanner] bounds in scanner?.previewLayer.frame = bounds }
      root.onRemoved = { [weak self] in
        self?.dispose()
        onDispose(id)
      }
      scanner.onResult = { [weak self] result in
        self?.sink?(RcodeFlutterPlugin.resultMap(result).merging(["type": "result"]) { _, new in new })
      }
      scanner.onError = { [weak self] error in self?.sink?(["type": "error", "code": RcodeFlutterPlugin.errorCode(error)]) }
    }

    func view() -> UIView { root }
    func onListen(withArguments arguments: Any?, eventSink events: @escaping FlutterEventSink) -> FlutterError? { sink = events; return nil }
    func onCancel(withArguments arguments: Any?) -> FlutterError? { sink = nil; return nil }
    func start() { scanner.start() }
    func stop() { scanner.stop() }
    func setTorch(_ enabled: Bool) throws { try scanner.setTorchEnabled(enabled) }
    func dispose() { scanner.stop(); eventChannel.setStreamHandler(nil) }

  }

  private final class PreviewContainerView: UIView {
    var onLayout: ((CGRect) -> Void)?
    var onRemoved: (() -> Void)?
    private var wasAttachedToWindow = false
    override func layoutSubviews() {
      super.layoutSubviews()
      onLayout?(bounds)
    }
    override func didMoveToWindow() {
      super.didMoveToWindow()
      if window != nil {
        wasAttachedToWindow = true
      } else if wasAttachedToWindow {
        onRemoved?()
      }
    }
  }
}
