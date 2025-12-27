import AppKit
import SwiftUI

@MainActor
final class SubtitlesApp: NSObject, NSApplicationDelegate {
    private let config = AppConfig.load()
    private let store: SubtitleStore
    private let translator: TranslationBroker
    private var engine: SubtitleEngine?
    private var window: NSWindow?
    private var keyMonitor: Any?

    override init() {
        self.store = SubtitleStore(maxHistory: config.maxHistory)
        self.translator = TranslationBroker(store: store)
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        setupWindow()
        engine = SubtitleEngine(config: config, store: store, translator: translator)
        Task { @MainActor in
            await engine?.start()
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        Task { @MainActor in
            await engine?.stop()
        }
    }

    private func setupWindow() {
        NSApp.setActivationPolicy(.regular)
        let screenFrame = NSScreen.main?.visibleFrame ?? CGRect(x: 0, y: 0, width: 1200, height: 800)
        let width = min(1000, screenFrame.width * 0.9)
        let height: CGFloat = 260
        let originX = screenFrame.midX - width / 2.0
        let originY = screenFrame.minY + 40

        let styleMask: NSWindow.StyleMask = config.debugOverlay
            ? [.titled, .closable, .resizable, .miniaturizable]
            : [.borderless, .resizable]

        let window = NSWindow(
            contentRect: CGRect(x: originX, y: originY, width: width, height: height),
            styleMask: styleMask,
            backing: .buffered,
            defer: false
        )

        if config.debugOverlay {
            window.isOpaque = true
            window.backgroundColor = NSColor.windowBackgroundColor
            window.level = .normal
            window.hasShadow = true
            window.title = "Subtitles (Debug)"
            window.center()
        } else {
            window.isOpaque = false
            window.backgroundColor = .clear
            window.level = .floating
            window.hasShadow = false
            window.isMovableByWindowBackground = true
            window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
            window.titleVisibility = .hidden
            window.titlebarAppearsTransparent = true
        }

        let view = SubtitlesView(store: store, translator: translator, config: config)
        window.contentView = NSHostingView(rootView: view)
        window.makeKeyAndOrderFront(nil)
        window.orderFrontRegardless()
        NSApp.activate(ignoringOtherApps: true)
        self.window = window

        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown]) { event in
            if event.modifierFlags.contains(.control),
               let chars = event.charactersIgnoringModifiers?.lowercased(),
               chars == "q" {
                NSApp.terminate(nil)
                return nil
            }
            return event
        }
    }
}
