import AppKit

// Finder supplies the real app/folder icons and their accessible labels.
// Only static instructional artwork is rendered during packaging.
guard CommandLine.arguments.count == 2 else {
    fputs("Uso: swift scripts/dmg-background.swift <background.png>\n", stderr)
    exit(2)
}
let width: CGFloat = 800
let height: CGFloat = 600
let image = NSImage(size: NSSize(width: width, height: height))
image.lockFocus()
NSColor(calibratedWhite: 0.97, alpha: 1).setFill()
NSRect(x: 0, y: 0, width: width, height: height).fill()
let ink = NSColor(calibratedWhite: 0.15, alpha: 1)
let secondary = NSColor(calibratedWhite: 0.32, alpha: 1)
let accent = NSColor(calibratedRed: 0.42, green: 0.48, blue: 0.95, alpha: 1)
func text(_ value: String, x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat,
          size: CGFloat, weight: NSFont.Weight = .regular, color: NSColor = ink) {
    let paragraph = NSMutableParagraphStyle()
    paragraph.lineSpacing = 3
    (value as NSString).draw(in: NSRect(x: x, y: 600 - y - height, width: width, height: height),
        withAttributes: [.font: NSFont.systemFont(ofSize: size, weight: weight),
                         .foregroundColor: color, .paragraphStyle: paragraph])
}
text("AMSpace", x: 36, y: 28, width: 728, height: 46, size: 32, weight: .semibold)
text("Arrastra a Aplicaciones. Abre la copia instalada.",
     x: 38, y: 79, width: 724, height: 24, size: 14, color: secondary)
text("Drag to Applications. Then open the installed copy.",
     x: 38, y: 104, width: 724, height: 24, size: 14, color: secondary)
let arrow = NSBezierPath()
arrow.lineWidth = 4
arrow.lineCapStyle = .round
arrow.move(to: NSPoint(x: 338, y: height - 210))
arrow.line(to: NSPoint(x: 462, y: height - 210))
arrow.move(to: NSPoint(x: 445, y: height - 193))
arrow.line(to: NSPoint(x: 462, y: height - 210))
arrow.line(to: NSPoint(x: 445, y: height - 227))
accent.setStroke()
arrow.stroke()
text("1. Abre la copia / Open installed app", x: 38, y: 316, width: 344, height: 28, size: 15, weight: .semibold)
text("Expulsa el disco. Abre Aplicaciones/AMSpace.app. Clic derecho en el indicador de la barra → Ajustes.",
     x: 38, y: 352, width: 344, height: 47, size: 12, color: secondary)
text("Eject this disk. Open Applications/AMSpace.app. Right-click the menu bar indicator → Settings.",
     x: 38, y: 404, width: 344, height: 47, size: 12, color: secondary)
text("2. Accesibilidad / Accessibility", x: 420, y: 316, width: 342, height: 28, size: 15, weight: .semibold)
text("Ajustes del Sistema → Privacidad y seguridad → Accesibilidad. Activa AMSpace; usa + si no aparece.",
     x: 420, y: 352, width: 342, height: 47, size: 12, color: secondary)
text("System Settings → Privacy & Security → Accessibility. Enable AMSpace; use + if it is missing.",
     x: 420, y: 404, width: 342, height: 47, size: 12, color: secondary)
NSColor(calibratedRed: 0.91, green: 0.93, blue: 0.98, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 24, y: 20, width: 550, height: 118), xRadius: 12, yRadius: 12).fill()
text("Firma ad hoc, sin notarización / Ad hoc signed, not notarized",
     x: 40, y: 470, width: 518, height: 24, size: 12, weight: .semibold)
text("Consulta «Léeme primero». Solo descarga oficial de confianza. No ignores malware ni desactives Gatekeeper o SIP.",
     x: 40, y: 502, width: 518, height: 32, size: 10.5, color: secondary)
text("Read «Léeme primero». Trust official downloads only. Never ignore malware warnings or disable Gatekeeper or SIP.",
     x: 40, y: 542, width: 518, height: 32, size: 10.5, color: secondary)
image.unlockFocus()
guard let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:]) else {
    fputs("No se pudo dibujar el fondo del instalador.\n", stderr)
    exit(1)
}
try png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]), options: .atomic)
