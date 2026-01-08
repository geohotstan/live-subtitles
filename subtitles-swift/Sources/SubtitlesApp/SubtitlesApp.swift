import AppKit
import SwiftUI

@MainActor
final class SubtitlesApp: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private var startStopItem: NSMenuItem?
    private var settingsObserver: NSObjectProtocol?
    private var spaceObserver: NSObjectProtocol?
    private var screenObserver: NSObjectProtocol?
    private var keyMonitor: Any?
    private var localKeyMonitor: Any?

    private var overlayWindow: NSWindow?
    private var preferencesWindow: NSWindow?
    private var engine: SubtitleEngine?

    private var config: AppConfig
    private let store: SubtitleStore
    private let translator: TranslationBroker

    private var isRunning = false

    override init() {
        let config = AppConfig.load()
        self.config = config
        self.store = SubtitleStore(maxHistory: config.maxHistory)
        self.translator = TranslationBroker(store: store)
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        setupStatusItem()
        installQuitKeyHandler()
        observeSettingsChanges()
        observeSpaceChanges()
    }

    func applicationWillTerminate(_ notification: Notification) {
        if let settingsObserver {
            NotificationCenter.default.removeObserver(settingsObserver)
        }
        if let spaceObserver {
            NSWorkspace.shared.notificationCenter.removeObserver(spaceObserver)
        }
        if let screenObserver {
            NotificationCenter.default.removeObserver(screenObserver)
        }
        removeQuitKeyHandler()
        Task { @MainActor in
            await stopSubtitles()
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    private func setupStatusItem() {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = item.button {
            if let image = NSImage(systemSymbolName: "captions.bubble", accessibilityDescription: "Subtitles") {
                image.isTemplate = true
                button.image = image
            } else {
                button.title = "Subtitles"
            }
            button.toolTip = "Subtitles"
        }

        let menu = NSMenu()
        let startStop = NSMenuItem(title: "Start Subtitles", action: #selector(toggleSubtitles), keyEquivalent: "")
        startStop.target = self
        menu.addItem(startStop)
        menu.addItem(.separator())

        let settings = NSMenuItem(title: "Settings…", action: #selector(openPreferences), keyEquivalent: ",")
        settings.target = self
        menu.addItem(settings)

        let about = NSMenuItem(title: "About Subtitles", action: #selector(showAbout), keyEquivalent: "")
        about.target = self
        menu.addItem(about)
        menu.addItem(.separator())

        let quit = NSMenuItem(title: "Quit Subtitles", action: #selector(quitApp), keyEquivalent: "q")
        quit.keyEquivalentModifierMask = [.control]
        quit.target = self
        menu.addItem(quit)

        item.menu = menu
        statusItem = item
        startStopItem = startStop
        updateMenuState()
    }

    private func observeSettingsChanges() {
        settingsObserver = NotificationCenter.default.addObserver(
            forName: UserDefaults.didChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.reloadConfigAndRestartIfNeeded()
            }
        }
    }

    private func observeSpaceChanges() {
        spaceObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.activeSpaceDidChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.refreshOverlayWindow()
            }
        }

        screenObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.refreshOverlayWindow()
            }
        }
    }

    private func reloadConfigAndRestartIfNeeded() {
        config = AppConfig.load()
        if isRunning {
            Task { @MainActor in
                await restartSubtitles()
            }
        }
    }

    @objc private func toggleSubtitles() {
        if isRunning {
            Task { @MainActor in
                await stopSubtitles()
            }
        } else {
            Task { @MainActor in
                await startSubtitles()
            }
        }
    }

    @objc private func openPreferences() {
        if preferencesWindow == nil {
            preferencesWindow = makePreferencesWindow()
        }
        preferencesWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc private func showAbout() {
        NSApp.activate(ignoringOtherApps: true)
        NSApp.orderFrontStandardAboutPanel(nil)
    }

    @objc private func quitApp() {
        NSApp.terminate(nil)
    }

    private func updateMenuState() {
        startStopItem?.title = isRunning ? "Stop Subtitles" : "Start Subtitles"
        startStopItem?.state = isRunning ? .on : .off
    }

    @MainActor
    private func startSubtitles() async {
        guard !isRunning else { return }
        isRunning = true
        updateMenuState()

        resetStore()
        buildOverlayWindow()

        let engine = SubtitleEngine(config: config, store: store, translator: translator)
        self.engine = engine
        await engine.start()
    }

    @MainActor
    private func stopSubtitles() async {
        guard isRunning else { return }
        isRunning = false
        updateMenuState()

        if let engine {
            await engine.stop()
        }
        engine = nil

        overlayWindow?.orderOut(nil)
        overlayWindow = nil
        store.updateStatus("Stopped.")
    }

    @MainActor
    private func restartSubtitles() async {
        await stopSubtitles()
        await startSubtitles()
    }

    @MainActor
    private func resetStore() {
        store.lines.removeAll()
        store.partialOriginal = ""
        store.partialEnglish = ""
        store.partialChinese = ""
        store.updateStatus("Starting…")
    }

    @MainActor
    private func buildOverlayWindow() {
        guard let screen = NSScreen.main ?? NSScreen.screens.first else { return }
        let rect = overlayFrame(for: screen)

        let window = NSWindow(
            contentRect: rect,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = false
        window.level = .screenSaver
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        window.ignoresMouseEvents = true
        window.isReleasedWhenClosed = false

        let view = SubtitlesView(store: store, translator: translator, config: config)
        window.contentView = NSHostingView(rootView: view)

        overlayWindow = window
        window.orderFrontRegardless()
    }

    @MainActor
    private func refreshOverlayWindow() {
        guard isRunning else { return }
        guard let window = overlayWindow else {
            buildOverlayWindow()
            return
        }

        if let screen = window.screen ?? NSScreen.main ?? NSScreen.screens.first {
            let rect = overlayFrame(for: screen)
            if window.frame != rect {
                window.setFrame(rect, display: true)
            }
        }

        window.orderFrontRegardless()
    }

    private func overlayFrame(for screen: NSScreen) -> NSRect {
        let frame = screen.frame
        let height = min(260, frame.height * 0.3)
        return NSRect(x: frame.minX, y: frame.minY, width: frame.width, height: height)
    }

    @MainActor
    private func makePreferencesWindow() -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 520, height: 520),
            styleMask: [.titled, .closable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Subtitles Settings"
        window.isReleasedWhenClosed = false
        window.center()
        window.contentView = NSHostingView(rootView: PreferencesView())
        return window
    }

    private func installQuitKeyHandler() {
        localKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            if event.modifierFlags.contains(.control),
               event.charactersIgnoringModifiers?.lowercased() == "q" {
                self?.quitApp()
                return nil
            }
            return event
        }

        keyMonitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) { [weak self] event in
            if event.modifierFlags.contains(.control),
               event.charactersIgnoringModifiers?.lowercased() == "q" {
                self?.quitApp()
            }
        }
    }

    private func removeQuitKeyHandler() {
        if let keyMonitor {
            NSEvent.removeMonitor(keyMonitor)
        }
        if let localKeyMonitor {
            NSEvent.removeMonitor(localKeyMonitor)
        }
        keyMonitor = nil
        localKeyMonitor = nil
    }
}
