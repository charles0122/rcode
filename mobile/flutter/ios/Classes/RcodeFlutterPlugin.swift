import AVFoundation
import Flutter
import RCodeApple
import UIKit

public class RcodeFlutterPlugin: NSObject, FlutterPlugin {
  private let registrar: FlutterPluginRegistrar
  private var views: [Int64: ScannerView] = [:]
  private var nextScannerId: Int64 = 1_000_000

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
    case "createScanner":
      let id = nextScannerId
      nextScannerId += 1
      let scanner = ScannerView(frame: .zero, id: id, registrar: registrar) { [weak self] id in
        self?.views.removeValue(forKey: id)
      }
      views[id] = scanner
      result(["viewId": id, "textureId": scanner.textureId])
    case "disposeScanner":
      guard let id = (args["viewId"] as? NSNumber)?.int64Value else {
        result(FlutterError(code: "invalidArgument", message: "Missing scanner id", details: nil)); return
      }
      views.removeValue(forKey: id)?.dispose()
      result(nil)
    case "encodePlan":
      guard let payload = args["payload"] as? FlutterStandardTypedData,
            let spokes = (args["spokes"] as? NSNumber)?.uint32Value,
            let eccRaw = (args["errorCorrection"] as? NSNumber)?.uint32Value,
            let ecc = ErrorCorrection(rawValue: eccRaw),
            let styleRaw = (args["style"] as? NSNumber)?.uint32Value,
            let style = RCodeStyle(rawValue: styleRaw) else {
        result(FlutterError(code: "invalidArgument", message: "Invalid encoding options", details: nil)); return
      }
      do {
        let plan = try RCodeEncoder().encode(
          payload.data,
          profile: Profile(spokes: spokes, errorCorrection: ecc),
          style: style
        )
        result(Self.planMap(plan))
      } catch let error as ScanError {
        result(FlutterError(code: Self.errorCode(error), message: error.localizedDescription, details: nil))
      } catch {
        result(FlutterError(code: "internal", message: error.localizedDescription, details: nil))
      }
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
    case "textureId":
      guard let view else { result(FlutterError(code: "not_ready", message: "Preview is not ready", details: nil)); return }
      result(view.textureId)
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

  private static func planMap(_ plan: RenderPlan) -> [String: Any] {
    ["version": Int(plan.version),
     "viewport": plan.viewport,
     "logoRegion": [plan.logoCenter.x, plan.logoCenter.y, plan.logoRadius],
     "commands": plan.commands.map { command in
       ["kind": Int(command.kind.rawValue),
        "paint": Int(command.paint.rawValue),
        "values": command.values]
     }]
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
      let view = ScannerView(frame: frame, id: viewId, registrar: plugin!.registrar) { [weak plugin] id in
        plugin?.views.removeValue(forKey: id)
      }
      plugin?.views[viewId] = view
      return view
    }
  }

  private final class ScannerView: NSObject, FlutterPlatformView, FlutterStreamHandler {
    private let root: PreviewContainerView
    private let scanner: RCodeScanner
    private let eventChannel: FlutterEventChannel
    private let texture = ScannerTexture()
    private let textureRegistry: FlutterTextureRegistry
    let textureId: Int64
    private var sink: FlutterEventSink?
    private let id: Int64
    private var disposed = false

    init(frame: CGRect, id: Int64, registrar: FlutterPluginRegistrar, onDispose: @escaping (Int64) -> Void) {
      self.id = id
      let root = PreviewContainerView(frame: frame)
      self.root = root
      scanner = RCodeScanner(previewLayer: root.previewLayer)
      textureRegistry = registrar.textures()
      textureId = textureRegistry.register(texture)
      eventChannel = FlutterEventChannel(name: "rcode_flutter/events/\(id)", binaryMessenger: registrar.messenger())
      super.init()
      eventChannel.setStreamHandler(self)
      root.onRemoved = { [weak self] in
        self?.dispose()
        onDispose(id)
      }
      scanner.onResult = { [weak self] result in
        self?.sink?(RcodeFlutterPlugin.resultMap(result).merging(["type": "result"]) { _, new in new })
      }
      scanner.onError = { [weak self] error in self?.sink?(["type": "error", "code": RcodeFlutterPlugin.errorCode(error)]) }
      scanner.onFrame = { [weak self] pixelBuffer in
        guard let self else { return }
        texture.update(pixelBuffer)
        DispatchQueue.main.async { [weak self] in
          guard let self, !disposed else { return }
          textureRegistry.textureFrameAvailable(textureId)
        }
      }
    }

    func view() -> UIView { root }
    func onListen(withArguments arguments: Any?, eventSink events: @escaping FlutterEventSink) -> FlutterError? { sink = events; return nil }
    func onCancel(withArguments arguments: Any?) -> FlutterError? { sink = nil; return nil }
    func start() { scanner.start() }
    func stop() { scanner.stop() }
    func setTorch(_ enabled: Bool) throws { try scanner.setTorchEnabled(enabled) }
    func dispose() {
      guard !disposed else { return }
      disposed = true
      scanner.stop()
      scanner.onFrame = nil
      eventChannel.setStreamHandler(nil)
      textureRegistry.unregisterTexture(textureId)
    }

  }

  private final class ScannerTexture: NSObject, FlutterTexture {
    private let lock = NSLock()
    private var latestPixelBuffer: CVPixelBuffer?

    func update(_ pixelBuffer: CVPixelBuffer) {
      lock.lock()
      latestPixelBuffer = pixelBuffer
      lock.unlock()
    }

    func copyPixelBuffer() -> Unmanaged<CVPixelBuffer>? {
      lock.lock()
      let pixelBuffer = latestPixelBuffer
      latestPixelBuffer = nil
      lock.unlock()
      return pixelBuffer.map(Unmanaged.passRetained)
    }
  }

  private final class PreviewContainerView: UIView {
    override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }
    var previewLayer: AVCaptureVideoPreviewLayer { layer as! AVCaptureVideoPreviewLayer }
    var onRemoved: (() -> Void)?
    private var wasAttachedToWindow = false
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
