// Export generated chroma-key teaching sprites without changing their sRGB palette.
import Cocoa

let root = URL(fileURLWithPath: CommandLine.arguments[1])
let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
let bitmapInfo = CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue

for name in ["pointer-up", "pointer-level"] {
    let source = NSBitmapImageRep(data: try Data(contentsOf: root.appendingPathComponent("assets/pet/teaching-source/\(name).png")))!
    precondition(source.bitsPerSample == 8 && !source.isPlanar && [3, 4].contains(source.samplesPerPixel), "Expected an 8-bit RGB(A) source")
    let width = source.pixelsWide, height = source.pixelsHigh
    let stride = source.bitsPerPixel / 8
    let input = source.bitmapData!
    // colorAt exposes NSCalibratedRGBColorSpace for these PNGs. Converting those
    // NSColors to deviceRGB lifts midtones. Key encoded bytes and label sRGB.
    func rgb(_ x: Int, _ y: Int) -> (Double, Double, Double) {
        let offset = y * source.bytesPerRow + x * stride
        return (Double(input[offset]) / 255, Double(input[offset + 1]) / 255, Double(input[offset + 2]) / 255)
    }
    var pixels = [UInt8](repeating: 0, count: width * height * 4)
    for y in 0..<height { for x in 0..<width {
        var color = rgb(x, y)
        let excess = min(color.0, color.2) - color.1
        let sourceAlpha = source.hasAlpha ? Double(input[y * source.bytesPerRow + x * stride + 3]) / 255 : 1
        let alpha = sourceAlpha * (1 - min(1, max(0, (excess - 0.16) / 0.34)))
        if alpha > 0 && excess > 0.08 {
            var found = false
            for radius in 1...4 {
                if found { break }
                for dy in -radius...radius {
                    if found { break }
                    for dx in -radius...radius {
                        let sx = x + dx, sy = y + dy
                        if sx < 0 || sy < 0 || sx >= width || sy >= height { continue }
                        let candidate = rgb(sx, sy)
                        if min(candidate.0, candidate.2) - candidate.1 < 0.08 {
                            color = candidate; found = true; break
                        }
                    }
                }
            }
        }
        let offset = (y * width + x) * 4
        pixels[offset] = UInt8((color.0 * alpha * 255).rounded())
        pixels[offset + 1] = UInt8((color.1 * alpha * 255).rounded())
        pixels[offset + 2] = UInt8((color.2 * alpha * 255).rounded())
        pixels[offset + 3] = UInt8((alpha * 255).rounded())
    } }
    let provider = CGDataProvider(data: Data(pixels) as CFData)!
    let sprite = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                         bytesPerRow: width * 4, space: colorSpace, bitmapInfo: CGBitmapInfo(rawValue: bitmapInfo),
                         provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)!
    for right in [false, true] {
        let context = CGContext(data: nil, width: 1440, height: 1400, bitsPerComponent: 8,
                                bytesPerRow: 1440 * 4, space: colorSpace, bitmapInfo: bitmapInfo)!
        context.interpolationQuality = .high
        if right { context.translateBy(x: 1440, y: 0); context.scaleBy(x: -1, y: 1) }
        context.draw(sprite, in: CGRect(x: 0, y: 0, width: 1440, height: 1400))
        let out = NSBitmapImageRep(cgImage: context.makeImage()!)
        let suffix = right ? "-right" : ""
        try out.representation(using: .png, properties: [:])!.write(to: root.appendingPathComponent("assets/pet/frames/teaching-\(name)\(suffix).png"))
    }
}
print("Exported 4 transparent 1440x1400 teaching frames with source sRGB colors preserved")
