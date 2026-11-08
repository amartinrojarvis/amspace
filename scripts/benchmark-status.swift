// Isolated AppKit reproduction of status-item snapshot feedback.
// swift scripts/benchmark-status.swift [--baseline]
// Temporarily adds four numbered segments to the menu bar, then exits.
import AppKit
import Darwin

final class StatusSegment: NSButton {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override func draw(_ dirtyRect: NSRect) {
        let title = attributedTitle
        let size = title.size()
        title.draw(at: NSPoint(
            x: bounds.minX + (bounds.width - size.width) / 2,
            y: bounds.minY + (bounds.height - size.height) / 2
        ))
    }
}

final class BaselineSegment: NSButton {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let item = NSStatusBar.system.statusItem(withLength: 96)
let parent = item.button!
parent.title = ""
let baseline = CommandLine.arguments.contains("--baseline")
for index in 0..<4 {
    let segment = baseline
        ? BaselineSegment(title: "", target: nil, action: nil)
        : StatusSegment(title: "", target: nil, action: nil)
    segment.isBordered = false
    segment.refusesFirstResponder = true
    segment.frame = NSRect(x: index * 24, y: 0, width: 24, height: 22)
    segment.attributedTitle = NSAttributedString(string: "\(index + 1)", attributes: [
        .font: NSFont.monospacedDigitSystemFont(ofSize: 13, weight: .medium),
        .foregroundColor: NSColor.labelColor.withAlphaComponent(index == 0 ? 1 : 0.38),
    ])
    parent.addSubview(segment)
}

func cpuSeconds(_ sample: rusage) -> Double {
    Double(sample.ru_utime.tv_sec + sample.ru_stime.tv_sec)
        + Double(sample.ru_utime.tv_usec + sample.ru_stime.tv_usec) / 1_000_000
}
var initial = rusage()
var start: TimeInterval = 0
DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
    getrusage(RUSAGE_SELF, &initial)
    start = ProcessInfo.processInfo.systemUptime
}
DispatchQueue.main.asyncAfter(deadline: .now() + 7) {
    var final = rusage()
    getrusage(RUSAGE_SELF, &final)
    let elapsed = ProcessInfo.processInfo.systemUptime - start
    let report: [String: Any] = [
        "baseline": baseline,
        "seconds": elapsed,
        "cpu_percent_one_core": 100 * (cpuSeconds(final) - cpuSeconds(initial)) / elapsed,
    ]
    let json = try! JSONSerialization.data(withJSONObject: report, options: .sortedKeys)
    print(String(data: json, encoding: .utf8)!)
    app.terminate(nil)
}
app.run()
