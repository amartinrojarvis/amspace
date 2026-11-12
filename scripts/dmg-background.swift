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
text("Arrastra la app a Aplicaciones. Después, abre la copia instalada.",
     x: 38, y: 83, width: 724, height: 30, size: 15, color: secondary)
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
text("1. Abre la copia instalada", x: 38, y: 326, width: 344, height: 25, size: 16, weight: .semibold)
text("Expulsa este disco y abre Aplicaciones/AMSpace.app. Vive en la barra de menús: clic derecho en el indicador → Ajustes.",
     x: 38, y: 360, width: 344, height: 76, size: 12.5, color: secondary)
text("2. Concede Accesibilidad", x: 420, y: 326, width: 342, height: 25, size: 16, weight: .semibold)
text("Ajustes del Sistema → Privacidad y seguridad → Accesibilidad. Activa AMSpace; si falta, usa + y elige la copia de Aplicaciones.",
     x: 420, y: 360, width: 342, height: 76, size: 12.5, color: secondary)
NSColor(calibratedRed: 0.91, green: 0.93, blue: 0.98, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 24, y: 26, width: 550, height: 124), xRadius: 12, yRadius: 12).fill()
text("Seguridad: firma ad hoc, sin notarización", x: 40, y: 466, width: 518, height: 25, size: 13, weight: .semibold)
text("Si macOS bloquea la apertura, consulta «Léeme primero». Autoriza solo una descarga oficial en la que confíes. No ignores avisos de malware ni desactives Gatekeeper o SIP.",
     x: 40, y: 498, width: 518, height: 66, size: 11.5, color: secondary)
image.unlockFocus()
guard let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:]) else {
    fputs("No se pudo dibujar el fondo del instalador.\n", stderr)
    exit(1)
}
try png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]), options: .atomic)
