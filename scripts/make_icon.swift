// Rasterizes the OPS icon with CoreGraphics (no external deps).
// Usage: swift scripts/make_icon.swift assets/icon.png
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let outPath = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "assets/icon.png"
let S: CGFloat = 1024

func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> CGColor {
    CGColor(red: r / 255, green: g / 255, blue: b / 255, alpha: a)
}

let cs = CGColorSpaceCreateDeviceRGB()
let ctx = CGContext(
    data: nil, width: Int(S), height: Int(S), bitsPerComponent: 8, bytesPerRow: 0,
    space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
)!
ctx.translateBy(x: 0, y: S)
ctx.scaleBy(x: 1, y: -1)
ctx.setAllowsAntialiasing(true)
ctx.setShouldAntialias(true)

let tile = CGPath(
    roundedRect: CGRect(x: 0, y: 0, width: S, height: S), cornerWidth: 228, cornerHeight: 228,
    transform: nil)

// Background gradient.
ctx.saveGState()
ctx.addPath(tile)
ctx.clip()
let bg = CGGradient(
    colorsSpace: cs,
    colors: [rgb(16, 25, 35), rgb(12, 18, 25), rgb(8, 11, 16)] as CFArray,
    locations: [0, 0.55, 1])!
ctx.drawLinearGradient(
    bg, start: CGPoint(x: 0, y: 0), end: CGPoint(x: S, y: S), options: [])

// Soft halo behind the mark.
let halo = CGGradient(
    colorsSpace: cs,
    colors: [rgb(76, 195, 138, 0.30), rgb(76, 195, 138, 0)] as CFArray,
    locations: [0, 1])!
ctx.drawRadialGradient(
    halo, startCenter: CGPoint(x: 512, y: 430), startRadius: 0,
    endCenter: CGPoint(x: 512, y: 430), endRadius: 560, options: [])
ctx.restoreGState()

// Inner border.
ctx.addPath(CGPath(
    roundedRect: CGRect(x: 22, y: 22, width: 980, height: 980), cornerWidth: 208,
    cornerHeight: 208, transform: nil))
ctx.setStrokeColor(rgb(34, 48, 63))
ctx.setLineWidth(6)
ctx.strokePath()

// Poker-chip ring.
ctx.setLineWidth(30)
ctx.setStrokeColor(rgb(34, 48, 63))
ctx.strokeEllipse(in: CGRect(x: 512 - 372, y: 512 - 372, width: 744, height: 744))

ctx.setLineCap(.round)
ctx.setLineDash(phase: 0, lengths: [64, 108])
ctx.beginPath()
let ring = CGGradient(
    colorsSpace: cs,
    colors: [rgb(111, 240, 180), rgb(76, 195, 138), rgb(74, 155, 255)] as CFArray,
    locations: [0, 0.55, 1])!
ctx.saveGState()
ctx.beginPath()
ctx.addEllipse(in: CGRect(x: 512 - 372, y: 512 - 372, width: 744, height: 744))
ctx.replacePathWithStrokedPath()
ctx.clip()
ctx.drawLinearGradient(
    ring, start: CGPoint(x: 140, y: 140), end: CGPoint(x: 884, y: 884), options: [])
ctx.restoreGState()
// Re-stroke the dashed ring so the gradient is visible on the dashes.
ctx.saveGState()
ctx.beginPath()
ctx.addEllipse(in: CGRect(x: 512 - 372, y: 512 - 372, width: 744, height: 744))
ctx.replacePathWithStrokedPath()
ctx.clip()
ctx.drawLinearGradient(
    ring, start: CGPoint(x: 140, y: 140), end: CGPoint(x: 884, y: 884), options: [])
ctx.restoreGState()

// Thin inner ring.
ctx.setLineDash(phase: 0, lengths: [])
ctx.setStrokeColor(rgb(27, 37, 49))
ctx.setLineWidth(8)
ctx.strokeEllipse(in: CGRect(x: 512 - 330, y: 512 - 330, width: 660, height: 660))

// Spade.
let spade = CGMutablePath()
spade.move(to: CGPoint(x: 512, y: 150))
spade.addCurve(
    to: CGPoint(x: 286, y: 540), control1: CGPoint(x: 512, y: 150),
    control2: CGPoint(x: 286, y: 368))
spade.addCurve(
    to: CGPoint(x: 458, y: 712), control1: CGPoint(x: 286, y: 646),
    control2: CGPoint(x: 360, y: 712))
spade.addCurve(
    to: CGPoint(x: 512, y: 680), control1: CGPoint(x: 496, y: 712),
    control2: CGPoint(x: 512, y: 680))
spade.addCurve(
    to: CGPoint(x: 566, y: 712), control1: CGPoint(x: 512, y: 680),
    control2: CGPoint(x: 528, y: 712))
spade.addCurve(
    to: CGPoint(x: 738, y: 540), control1: CGPoint(x: 664, y: 712),
    control2: CGPoint(x: 738, y: 646))
spade.addCurve(
    to: CGPoint(x: 512, y: 150), control1: CGPoint(x: 738, y: 368),
    control2: CGPoint(x: 512, y: 150))
spade.closeSubpath()

let stem = CGMutablePath()
stem.move(to: CGPoint(x: 470, y: 620))
stem.addCurve(
    to: CGPoint(x: 450, y: 810), control1: CGPoint(x: 466, y: 700),
    control2: CGPoint(x: 452, y: 760))
stem.addLine(to: CGPoint(x: 574, y: 810))
stem.addCurve(
    to: CGPoint(x: 554, y: 620), control1: CGPoint(x: 572, y: 760),
    control2: CGPoint(x: 558, y: 700))
stem.closeSubpath()

let mark = CGGradient(
    colorsSpace: cs,
    colors: [rgb(111, 240, 180), rgb(76, 195, 138), rgb(74, 155, 255)] as CFArray,
    locations: [0, 0.55, 1])!

func paint(_ path: CGPath, glow: Bool) {
    ctx.saveGState()
    if glow {
        ctx.setShadow(offset: .zero, blur: 34, color: rgb(76, 195, 138, 0.55))
    }
    ctx.addPath(path)
    ctx.clip()
    ctx.drawLinearGradient(
        mark, start: CGPoint(x: 280, y: 160), end: CGPoint(x: 744, y: 820), options: [])
    ctx.restoreGState()
}

// Two fills: the stem first, then the body on top (same gradient, no seam).
paint(stem, glow: true)
paint(spade, glow: false)

// Decision tree inside the spade.
let ink = rgb(8, 17, 13, 0.92)
ctx.setStrokeColor(ink)
ctx.setLineWidth(14)
ctx.setLineCap(.round)
ctx.move(to: CGPoint(x: 512, y: 452))
ctx.addLine(to: CGPoint(x: 424, y: 566))
ctx.move(to: CGPoint(x: 512, y: 452))
ctx.addLine(to: CGPoint(x: 600, y: 566))
ctx.strokePath()

func dot(_ x: CGFloat, _ y: CGFloat, _ r: CGFloat, _ color: CGColor) {
    ctx.setFillColor(color)
    ctx.fillEllipse(in: CGRect(x: x - r, y: y - r, width: r * 2, height: r * 2))
}

dot(512, 452, 30, ink)
dot(424, 566, 24, ink)
dot(600, 566, 24, ink)
dot(512, 452, 13, rgb(184, 255, 221))
dot(424, 566, 10, rgb(184, 255, 221))
dot(600, 566, 10, rgb(184, 255, 221))

let image = ctx.makeImage()!
let url = URL(fileURLWithPath: outPath)
try? FileManager.default.removeItem(at: url)
let dest = CGImageDestinationCreateWithURL(
    url as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(dest, image, nil)
if !CGImageDestinationFinalize(dest) {
    FileHandle.standardError.write(Data("failed to write \(outPath)\n".utf8))
    exit(1)
}
print("wrote \(outPath)")
