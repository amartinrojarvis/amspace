// Build-time vector artwork. The application itself is entirely Rust.
import AppKit
let destination = CommandLine.arguments[1]
let size = 1024
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()
let outer = NSBezierPath(roundedRect: NSRect(x: 60, y: 60, width: 904, height: 904), xRadius: 210, yRadius: 210)
NSGradient(starting: NSColor(srgbRed: 0.16, green: 0.19, blue: 0.32, alpha: 1), ending: NSColor(srgbRed: 0.055, green: 0.065, blue: 0.11, alpha: 1))!.draw(in: outer, angle: -65)
NSColor(srgbRed: 0.45, green: 0.50, blue: 0.96, alpha: 0.24).setStroke()
outer.lineWidth = 3
outer.stroke()
let screen = NSBezierPath(roundedRect: NSRect(x: 210, y: 286, width: 604, height: 452), xRadius: 84, yRadius: 84)
NSColor(srgbRed: 0.70, green: 0.74, blue: 1, alpha: 1).setStroke()
screen.lineWidth = 23
screen.stroke()
let active = NSBezierPath(roundedRect: NSRect(x: 467, y: 449, width: 90, height: 126), xRadius: 29, yRadius: 29)
NSColor(srgbRed: 0.76, green: 0.80, blue: 1, alpha: 1).setFill()
active.fill()
NSColor(srgbRed: 0.48, green: 0.52, blue: 0.68, alpha: 1).setFill()
for x in [344, 644] { NSBezierPath(ovalIn: NSRect(x: x, y: 495, width: 34, height: 34)).fill() }
image.unlockFocus()
let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: destination))
