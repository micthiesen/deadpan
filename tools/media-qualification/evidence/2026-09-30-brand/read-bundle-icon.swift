import AppKit
import Foundation

let arguments = CommandLine.arguments
precondition(arguments.count == 3)
let path = arguments[1]
let destination = URL(fileURLWithPath: arguments[2])
let icon = NSWorkspace.shared.icon(forFile: path)
guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 1024, pixelsHigh: 1024, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
      let context = NSGraphicsContext(bitmapImageRep: bitmap) else {
    fatalError("Cannot allocate icon readback")
}
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = context
context.imageInterpolation = .high
icon.draw(in: NSRect(x: 0, y: 0, width: 1024, height: 1024), from: .zero, operation: .copy, fraction: 1)
NSGraphicsContext.restoreGraphicsState()
guard let png = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("Cannot encode icon readback")
}
try png.write(to: destination, options: .withoutOverwriting)
print("NSWorkspace icon readback: \(destination.path), \(bitmap.pixelsWide)x\(bitmap.pixelsHigh), \(png.count) bytes")
