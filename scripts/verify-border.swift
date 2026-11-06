// Read-only integration check. No screenshots, titles, window movement or focus changes.
// Usage: swift scripts/verify-border.swift (enable the border and focus another app).
import AppKit
let bundle = "io.github.amartinrojarvis.amspace"
guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: bundle).first,
      let front = NSWorkspace.shared.frontmostApplication, front.processIdentifier != app.processIdentifier,
      let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else {
    print("Focus another app with AMSpace running."); exit(2)
}
func frame(_ window: [String: Any]) -> CGRect? {
    guard let raw = window[kCGWindowBounds as String] as? [String: Any] else { return nil }
    return CGRect(dictionaryRepresentation: raw as CFDictionary)
}
let normal = list.filter { ($0[kCGWindowLayer as String] as? Int) == 0 && (frame($0)?.width ?? 0) > 40 && (frame($0)?.height ?? 0) > 40 }
guard let target = normal.first(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == front.processIdentifier }), let expected = frame(target) else {
    print("No visible normal target window."); exit(2)
}
let overlays = normal.filter { ($0[kCGWindowOwnerPID as String] as? Int32) == app.processIdentifier }
let match = overlays.first { frame($0) == expected }
print("target_id=\(target[kCGWindowNumber as String]!) frame=\(expected)")
if let overlay = match {
    print("PASS: border_id=\(overlay[kCGWindowNumber as String]!), exact compositor bounds (zero gap).")
} else {
    for overlay in overlays { print("border_candidate=\(overlay[kCGWindowNumber as String]!), frame=\(String(describing: frame(overlay)))") }
    print("FAIL: no AMSpace panel matches the front window."); exit(1)
}
