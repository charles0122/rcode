import Foundation
import RCodeCore
import UIKit

public struct LumaDecoder {
    public init() {}

    public func decode(bytes: UnsafeRawBufferPointer, width: Int, height: Int,
                       bytesPerRow: Int) throws -> ScanResult {
        guard width > 0, height > 0, width <= Int(UInt32.max), height <= Int(UInt32.max),
              bytesPerRow >= width, let baseAddress = bytes.baseAddress else {
            throw ScanError.invalidImage
        }

        var opaqueResult: OpaquePointer?
        let status = rcode_decode_luma8(
            baseAddress.assumingMemoryBound(to: UInt8.self), bytes.count,
            UInt32(width), UInt32(height), bytesPerRow, &opaqueResult
        )
        guard status == 0, let result = opaqueResult else {
            throw scanError(status)
        }
        defer { rcode_result_free(result) }

        var payloadPointer: UnsafePointer<UInt8>?
        var payloadLength = 0
        try check(rcode_result_payload(result, &payloadPointer, &payloadLength))
        var imageWidth: UInt32 = 0
        var imageHeight: UInt32 = 0
        try check(rcode_result_image_size(result, &imageWidth, &imageHeight))
        var spokes: UInt32 = 0
        var ecc: UInt32 = 0
        try check(rcode_result_profile(result, &spokes, &ecc))
        guard let errorCorrection = ErrorCorrection(rawValue: ecc) else { throw ScanError.internalPanic }
        var corrected = 0
        try check(rcode_result_corrected_bytes(result, &corrected))

        let landmarks = try (0..<4).map { index -> Landmark in
            var x = 0.0
            var y = 0.0
            try check(rcode_result_landmark(result, UInt32(index), &x, &y))
            return Landmark(x: x, y: y)
        }
        let payload = payloadPointer.map { Data(bytes: $0, count: payloadLength) } ?? Data()
        return ScanResult(payload: payload, imageWidth: imageWidth, imageHeight: imageHeight,
                          landmarks: landmarks,
                          profile: Profile(spokes: spokes, errorCorrection: errorCorrection),
                          correctedBytes: corrected)
    }

    /// Decodes one PNG or JPEG. UIKit normalizes image orientation before luma conversion.
    public func decode(imageData: Data) throws -> ScanResult {
        guard !imageData.isEmpty, imageData.count <= 8 * 1024 * 1024,
              let image = UIImage(data: imageData) else {
            throw ScanError.invalidImage
        }
        return try decode(image: image)
    }

    public func decode(image: UIImage) throws -> ScanResult {
        guard let source = image.cgImage else { throw ScanError.invalidImage }
        let swapsAxes: Bool
        switch image.imageOrientation {
        case .left, .leftMirrored, .right, .rightMirrored: swapsAxes = true
        default: swapsAxes = false
        }
        let width = swapsAxes ? source.height : source.width
        let height = swapsAxes ? source.width : source.height
        guard width >= 2, height >= 2,
              Int64(width) * Int64(height) <= 16_000_000 else {
            throw ScanError.invalidImage
        }

        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.opaque = true
        let normalized = UIGraphicsImageRenderer(
            size: CGSize(width: width, height: height),
            format: format
        ).image { context in
            UIColor.white.setFill()
            context.fill(CGRect(x: 0, y: 0, width: width, height: height))
            image.draw(in: CGRect(x: 0, y: 0, width: width, height: height))
        }
        guard let cgImage = normalized.cgImage else { throw ScanError.invalidImage }

        var luma = [UInt8](repeating: 0, count: width * height)
        let rendered = luma.withUnsafeMutableBytes { storage -> Bool in
            guard let context = CGContext(
                data: storage.baseAddress,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: width,
                space: CGColorSpaceCreateDeviceGray(),
                bitmapInfo: CGImageAlphaInfo.none.rawValue
            ) else { return false }
            context.draw(cgImage, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard rendered else { throw ScanError.invalidImage }
        return try luma.withUnsafeBytes {
            try decode(bytes: $0, width: width, height: height, bytesPerRow: width)
        }
    }

    private func check(_ status: UInt32) throws {
        guard status == 0 else { throw scanError(status) }
    }

    private func scanError(_ status: UInt32) -> ScanError {
        switch status {
        case 1: .invalidArgument
        case 2: .noCode
        case 3: .corruptCode
        case 4: .invalidImage
        default: .internalPanic
        }
    }
}
