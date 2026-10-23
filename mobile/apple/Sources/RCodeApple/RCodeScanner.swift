import AVFoundation
import Foundation
import UIKit

public final class RCodeScanner: NSObject, @unchecked Sendable {
    public let session = AVCaptureSession()
    public let previewLayer: AVCaptureVideoPreviewLayer
    public var onResult: ((ScanResult) -> Void)?
    public var onError: ((ScanError) -> Void)?

    private let sessionQueue = DispatchQueue(label: "com.rcode.camera.session")
    private let analysisQueue = DispatchQueue(label: "com.rcode.camera.analysis")
    private let decoder = LumaDecoder()
    private var configured = false
    private var videoOutput: AVCaptureVideoDataOutput?
    private let resultGate = ScanResultGate()
    private let scanIntent = ScanIntent()

    public override init() {
        previewLayer = AVCaptureVideoPreviewLayer(session: session)
        super.init()
        previewLayer.videoGravity = .resizeAspectFill
        NotificationCenter.default.addObserver(self, selector: #selector(willEnterForeground),
                                               name: UIApplication.willEnterForegroundNotification, object: nil)
        NotificationCenter.default.addObserver(self, selector: #selector(didEnterBackground),
                                               name: UIApplication.didEnterBackgroundNotification, object: nil)
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    public func start() {
        scanIntent.start()
        requestSessionStart()
    }

    public func stop() {
        scanIntent.stop()
        pauseSession()
    }

    public var isTorchAvailable: Bool {
        AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back)?.hasTorch == true
    }

    public func setTorchEnabled(_ enabled: Bool) throws {
        guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back),
              device.hasTorch else { throw ScanError.cameraUnavailable }
        try device.lockForConfiguration()
        defer { device.unlockForConfiguration() }
        device.torchMode = enabled ? .on : .off
    }

    public func setVideoOrientation(_ orientation: AVCaptureVideoOrientation) {
        sessionQueue.async { [weak self] in
            guard let self else { return }
            self.videoOutput?.connection(with: .video)?.videoOrientation = orientation
            self.previewLayer.connection?.videoOrientation = orientation
        }
    }

    /// Converts decoded image pixels to the preview layer. The rear camera connection stays unmirrored.
    public func previewPoint(for landmark: Landmark, in result: ScanResult) -> CGPoint {
        guard let devicePoint = PreviewCoordinateMapper.captureDevicePoint(for: landmark, in: result) else {
            return .zero
        }
        return previewLayer.layerPointConverted(fromCaptureDevicePoint: devicePoint)
    }

    private func requestSessionStart() {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized: beginSession()
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
                guard let self else { return }
                guard self.scanIntent.isRunning else { return }
                if granted { self.beginSession() }
                else { self.report(.permissionDenied) }
            }
        default: report(.permissionDenied)
        }
    }

    private func pauseSession() {
        resultGate.stop()
        sessionQueue.async { [weak self] in
            guard let self, self.session.isRunning else { return }
            self.session.stopRunning()
        }
    }

    private func beginSession() {
        sessionQueue.async { [weak self] in
            guard let self, self.scanIntent.isRunning else { return }
            do {
                try self.configureIfNeeded()
                self.resultGate.reset()
                if !self.session.isRunning { self.session.startRunning() }
            } catch let error as ScanError {
                self.report(error)
            } catch {
                self.report(.configuration(error.localizedDescription))
            }
        }
    }

    private func configureIfNeeded() throws {
        guard !configured else { return }
        guard let camera = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back) else {
            throw ScanError.cameraUnavailable
        }
        let input = try AVCaptureDeviceInput(device: camera)
        let output = AVCaptureVideoDataOutput()
        output.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange]
        output.alwaysDiscardsLateVideoFrames = true
        output.setSampleBufferDelegate(self, queue: analysisQueue)

        session.beginConfiguration()
        defer { session.commitConfiguration() }
        session.sessionPreset = .high
        guard session.canAddInput(input), session.canAddOutput(output) else {
            throw ScanError.configuration("The camera session could not be configured.")
        }
        session.addInput(input)
        session.addOutput(output)
        if let connection = output.connection(with: .video) {
            connection.videoOrientation = .portrait
            connection.isVideoMirrored = false
        }
        if let connection = previewLayer.connection {
            connection.videoOrientation = .portrait
            connection.isVideoMirrored = false
        }
        videoOutput = output
        configured = true
    }

    private func report(_ error: ScanError) {
        DispatchQueue.main.async { [weak self] in self?.onError?(error) }
    }

    @objc private func willEnterForeground() {
        if scanIntent.isRunning { requestSessionStart() }
    }

    @objc private func didEnterBackground() { pauseSession() }
}

extension RCodeScanner: AVCaptureVideoDataOutputSampleBufferDelegate {
    public func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer,
                              from connection: AVCaptureConnection) {
        guard resultGate.canDecode,
              let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        guard CVPixelBufferLockBaseAddress(pixelBuffer, .readOnly) == kCVReturnSuccess else { return }
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, .readOnly) }
        guard CVPixelBufferGetPlaneCount(pixelBuffer) > 0,
              let base = CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 0) else { return }
        let width = CVPixelBufferGetWidthOfPlane(pixelBuffer, 0)
        let height = CVPixelBufferGetHeightOfPlane(pixelBuffer, 0)
        let stride = CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 0)
        let bytes = UnsafeRawBufferPointer(start: base, count: stride * height)
        guard let result = try? decoder.decode(bytes: bytes, width: width, height: height,
                                               bytesPerRow: stride) else { return }
        if resultGate.claim() {
            DispatchQueue.main.async { [weak self] in
                guard let self, self.scanIntent.isRunning else { return }
                self.onResult?(result)
            }
        }
    }
}

extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
