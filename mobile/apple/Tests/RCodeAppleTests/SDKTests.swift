import AVFoundation
import CoreGraphics
import XCTest
@testable import RCodeApple

final class SDKTests: XCTestCase {
    func testScanModelsPreservePayloadAndMetadata() {
        let result = ScanResult(payload: Data([0, 1, 255]), imageWidth: 640, imageHeight: 480,
                               landmarks: [Landmark(x: 10, y: 20)],
                               profile: Profile(spokes: 54, errorCorrection: .quartile),
                               correctedBytes: 2)
        XCTAssertEqual(result.payload, Data([0, 1, 255]))
        XCTAssertEqual(result.landmarks, [Landmark(x: 10, y: 20)])
        XCTAssertEqual(result.profile, Profile(spokes: 54, errorCorrection: .quartile))
        XCTAssertEqual(result.correctedBytes, 2)
    }

    func testResultGateDeliversOnceAndResetsForNextScan() {
        let gate = ScanResultGate()
        XCTAssertFalse(gate.claim())
        gate.reset()
        XCTAssertTrue(gate.claim())
        XCTAssertFalse(gate.claim())
        gate.stop()
        XCTAssertFalse(gate.claim())
        gate.reset()
        XCTAssertTrue(gate.claim())
    }

    func testScanIntentDistinguishesBackgroundPauseFromExplicitStop() {
        let intent = ScanIntent()
        XCTAssertFalse(intent.isRunning)
        intent.start()
        XCTAssertTrue(intent.isRunning)
        intent.stop()
        XCTAssertFalse(intent.isRunning)
    }

    func testPreviewMapperNormalizesImagePixelLandmarks() {
        let result = ScanResult(payload: Data(), imageWidth: 1000, imageHeight: 500,
                                landmarks: [],
                                profile: Profile(spokes: 54, errorCorrection: .medium),
                                correctedBytes: 0)
        let point = PreviewCoordinateMapper.captureDevicePoint(
            for: Landmark(x: 250, y: 125), in: result
        )
        XCTAssertEqual(point?.x ?? -1, 0.25, accuracy: 0.001)
        XCTAssertEqual(point?.y ?? -1, 0.25, accuracy: 0.001)
        XCTAssertNil(PreviewCoordinateMapper.captureDevicePoint(
            for: Landmark(x: 0, y: 0),
            in: ScanResult(payload: Data(), imageWidth: 0, imageHeight: 0, landmarks: [],
                           profile: Profile(spokes: 54, errorCorrection: .medium), correctedBytes: 0)
        ))
    }

    func testDecoderRejectsInvalidLumaLayout() {
        let decoder = LumaDecoder()
        XCTAssertThrowsError(try decoder.decode(bytes: UnsafeRawBufferPointer(start: nil, count: 0),
                                                width: 0, height: 0, bytesPerRow: 0))
        XCTAssertThrowsError(try decoder.decode(imageData: Data()))
    }

    func testDecoderReadsEncodedImageData() throws {
        let url = try XCTUnwrap(Bundle.module.url(
            forResource: "current-v3", withExtension: "png", subdirectory: "Fixtures"
        ))
        let decoded = try LumaDecoder().decode(imageData: Data(contentsOf: url))
        XCTAssertEqual(decoded.payload, Data("apple-image-fixture".utf8))
        XCTAssertEqual(decoded.profile, Profile(spokes: 54, errorCorrection: .quartile))
        XCTAssertEqual(decoded.landmarks.count, 4)
    }
}
