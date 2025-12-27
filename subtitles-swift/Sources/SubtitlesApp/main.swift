import AppKit

let app = NSApplication.shared
let delegate = SubtitlesApp()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
