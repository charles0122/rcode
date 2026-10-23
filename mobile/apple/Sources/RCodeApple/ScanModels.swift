import AVFoundation
import CoreGraphics
import Foundation
import UIKit

public struct Landmark: Equatable, Sendable {
    /// Coordinates in the decoded luma image, with the origin at its upper-left.
    public let x: Double
    public let y: Double

    public init(x: Double, y: Double) {
        self.x = x
        self.y = y
    }
}

public enum ErrorCorrection: UInt32, Equatable, Sendable {
    case low = 0
    case medium = 1
    case quartile = 2
    case high = 3
}

public struct Profile: Equatable, Sendable {
    public let spokes: UInt32
    public let errorCorrection: ErrorCorrection

    public init(spokes: UInt32, errorCorrection: ErrorCorrection) {
        self.spokes = spokes
        self.errorCorrection = errorCorrection
    }
}

public struct ScanResult: Equatable, Sendable {
    public let payload: Data
    public let imageWidth: UInt32
    public let imageHeight: UInt32
    public let landmarks: [Landmark]
    public let profile: Profile
    public let correctedBytes: Int

    public init(payload: Data, imageWidth: UInt32, imageHeight: UInt32,
                landmarks: [Landmark], profile: Profile, correctedBytes: Int) {
        self.payload = payload
        self.imageWidth = imageWidth
        self.imageHeight = imageHeight
        self.landmarks = landmarks
        self.profile = profile
        self.correctedBytes = correctedBytes
    }
}

public enum ScanError: Error, Equatable, Sendable {
    case invalidArgument
    case noCode
    case corruptCode
    case invalidImage
    case internalPanic
    case cameraUnavailable
    case permissionDenied
    case configuration(String)
}

extension ScanError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .invalidArgument: "Invalid decoder input."
        case .noCode: "No RCode found."
        case .corruptCode: "The RCode is damaged."
        case .invalidImage: "The camera image is invalid."
        case .internalPanic: "The decoder failed unexpectedly."
        case .cameraUnavailable: "The rear camera is unavailable."
        case .permissionDenied: "Camera access is denied."
        case .configuration(let message): message
        }
    }
}

public enum CameraPermission {
    public static var status: AVAuthorizationStatus {
        AVCaptureDevice.authorizationStatus(for: .video)
    }

    public static func request(_ completion: @escaping (Bool) -> Void) {
        guard status == .notDetermined else {
            completion(status == .authorized)
            return
        }
        AVCaptureDevice.requestAccess(for: .video, completionHandler: completion)
    }
}

enum PreviewCoordinateMapper {
    static func captureDevicePoint(for landmark: Landmark, in result: ScanResult) -> CGPoint? {
        guard result.imageWidth > 0, result.imageHeight > 0 else { return nil }
        return CGPoint(x: landmark.x / Double(result.imageWidth),
                       y: landmark.y / Double(result.imageHeight))
    }
}

final class ScanResultGate {
    private let lock = NSLock()
    private var hasDelivered = false
    private var isActive = false

    func reset() {
        lock.withLock {
            hasDelivered = false
            isActive = true
        }
    }

    func stop() {
        lock.withLock { isActive = false }
    }

    var canDecode: Bool {
        lock.withLock { isActive && !hasDelivered }
    }

    func claim() -> Bool {
        lock.withLock {
            guard isActive, !hasDelivered else { return false }
            hasDelivered = true
            return true
        }
    }
}

final class ScanIntent {
    private let lock = NSLock()
    private var shouldRun = false

    var isRunning: Bool { lock.withLock { shouldRun } }
    func start() { lock.withLock { shouldRun = true } }
    func stop() { lock.withLock { shouldRun = false } }
}
