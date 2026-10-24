import CoreGraphics
import Foundation
import RCodeCore
import UIKit

public enum RCodeStyle: UInt32, Sendable {
    case lines = 0
    case dots = 1
    case rings = 2
}

public enum DrawKind: UInt32, Sendable {
    case circle = 0
    case line = 1
    case arc = 2
}

public enum DrawPaint: UInt32, Sendable {
    case background = 0
    case foreground = 1
}

public struct DrawCommand: Sendable {
    public let kind: DrawKind
    public let paint: DrawPaint
    public let values: [Double]
}

public struct RenderPlan: Sendable {
    public let version: UInt32
    public let viewport: Double
    public let logoCenter: CGPoint
    public let logoRadius: Double
    public let commands: [DrawCommand]
}

public struct RCodeEncoder {
    public init() {}

    public func encode(_ payload: Data, profile: Profile = Profile(
        spokes: 72, errorCorrection: .medium
    ), style: RCodeStyle = .lines) throws -> RenderPlan {
        var opaquePlan: OpaquePointer?
        let status = payload.withUnsafeBytes { bytes in
            rcode_encode_plan(
                bytes.bindMemory(to: UInt8.self).baseAddress,
                bytes.count,
                profile.spokes,
                profile.errorCorrection.rawValue,
                style.rawValue,
                &opaquePlan
            )
        }
        guard status == 0, let plan = opaquePlan else { throw scanError(status) }
        defer { rcode_plan_free(plan) }

        var version: UInt32 = 0
        try check(rcode_plan_version(plan, &version))
        guard version == 1 else { throw ScanError.internalPanic }
        var viewport = 0.0
        var logoX = 0.0
        var logoY = 0.0
        var logoRadius = 0.0
        try check(rcode_plan_metadata(
            plan, &viewport, &logoX, &logoY, &logoRadius
        ))

        var operationPointer: UnsafePointer<RCodeDrawOperation>?
        var operationCount = 0
        try check(rcode_plan_operations(plan, &operationPointer, &operationCount))
        let operations = operationPointer.map {
            UnsafeBufferPointer(start: $0, count: operationCount)
        } ?? UnsafeBufferPointer(start: nil, count: 0)
        let commands = try operations.map { operation -> DrawCommand in
            guard let kind = DrawKind(rawValue: operation.kind),
                  let paint = DrawPaint(rawValue: operation.paint) else {
                throw ScanError.internalPanic
            }
            var tuple = operation.values
            let values = withUnsafeBytes(of: &tuple) {
                Array($0.bindMemory(to: Double.self))
            }
            return DrawCommand(kind: kind, paint: paint, values: values)
        }
        return RenderPlan(
            version: version,
            viewport: viewport,
            logoCenter: CGPoint(x: logoX, y: logoY),
            logoRadius: logoRadius,
            commands: commands
        )
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

/// Thin CoreGraphics adapter. Protocol geometry remains entirely in Rust.
public enum RCodeRenderer {
    public static func image(
        plan: RenderPlan,
        size: CGFloat,
        foreground: UIColor = UIColor(red: 23 / 255, green: 59 / 255, blue: 50 / 255, alpha: 1),
        background: UIColor = .white,
        logo: UIImage? = nil
    ) -> UIImage {
        precondition(size > 0)
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.opaque = true
        return UIGraphicsImageRenderer(size: CGSize(width: size, height: size), format: format)
            .image { renderer in
                draw(
                    plan: plan,
                    in: renderer.cgContext,
                    bounds: CGRect(x: 0, y: 0, width: size, height: size),
                    foreground: foreground.cgColor,
                    background: background.cgColor,
                    logo: logo
                )
            }
    }

    public static func draw(
        plan: RenderPlan,
        in context: CGContext,
        bounds: CGRect,
        foreground: CGColor,
        background: CGColor,
        logo: UIImage? = nil
    ) {
        let side = min(bounds.width, bounds.height)
        let origin = CGPoint(
            x: bounds.minX + (bounds.width - side) / 2,
            y: bounds.minY + (bounds.height - side) / 2
        )
        let scale = side / plan.viewport
        context.saveGState()
        context.setFillColor(background)
        context.fill(bounds)
        context.translateBy(x: origin.x, y: origin.y)
        context.scaleBy(x: scale, y: scale)

        for command in plan.commands {
            let color = command.paint == .foreground ? foreground : background
            let values = command.values
            switch command.kind {
            case .circle:
                context.setFillColor(color)
                context.fillEllipse(in: CGRect(
                    x: values[0] - values[2], y: values[1] - values[2],
                    width: values[2] * 2, height: values[2] * 2
                ))
            case .line:
                if values[0] == values[2], values[1] == values[3] {
                    context.setFillColor(color)
                    context.fillEllipse(in: CGRect(
                        x: values[0] - values[4] / 2,
                        y: values[1] - values[4] / 2,
                        width: values[4], height: values[4]
                    ))
                } else {
                    context.setStrokeColor(color)
                    context.setLineWidth(values[4])
                    context.setLineCap(.round)
                    context.move(to: CGPoint(x: values[0], y: values[1]))
                    context.addLine(to: CGPoint(x: values[2], y: values[3]))
                    context.strokePath()
                }
            case .arc:
                context.setStrokeColor(color)
                context.setLineWidth(values[5])
                context.setLineCap(.round)
                context.setLineJoin(.round)
                let segments = max(2, Int(ceil(values[2] * abs(values[4]) / 1.5)))
                for index in 0...segments {
                    let angle = values[3] + values[4] * Double(index) / Double(segments)
                    let point = CGPoint(
                        x: values[0] + values[2] * cos(angle),
                        y: values[1] + values[2] * sin(angle)
                    )
                    if index == 0 { context.move(to: point) }
                    else { context.addLine(to: point) }
                }
                context.strokePath()
            }
        }

        if let cgLogo = logo?.cgImage, plan.logoRadius > 0 {
            context.saveGState()
            let rect = CGRect(
                x: plan.logoCenter.x - plan.logoRadius,
                y: plan.logoCenter.y - plan.logoRadius,
                width: plan.logoRadius * 2,
                height: plan.logoRadius * 2
            )
            context.addEllipse(in: rect)
            context.clip()
            context.draw(cgLogo, in: rect)
            context.restoreGState()
        }
        context.restoreGState()
    }
}
