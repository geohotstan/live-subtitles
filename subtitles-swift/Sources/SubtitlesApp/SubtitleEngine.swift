import AVFoundation
import Foundation
@preconcurrency import ScreenCaptureKit
import Speech

final class SubtitleEngine: NSObject, SCStreamOutput, SCStreamDelegate {
    private let config: AppConfig
    private let store: SubtitleStore
    private let translator: TranslationBroker

    private let audioQueue = DispatchQueue(label: "subtitles.audio.capture", qos: .userInitiated)
    private let converter: AudioPCMConverter

    private var stream: SCStream?
    private var transcriber: SpeechTranscriber?

    init(config: AppConfig, store: SubtitleStore, translator: TranslationBroker) {
        self.config = config
        self.store = store
        self.translator = translator
        self.converter = AudioPCMConverter(outputSampleRate: config.outputSampleRate, outputChannels: config.outputChannels)
        super.init()
    }

    @MainActor
    func start() async {
        store.updateStatus("Requesting speech recognition permission...")

        let authorized = await requestSpeechAuthorization()
        guard authorized else {
            store.updateStatus("Speech recognition permission denied. Enable it in System Settings > Privacy & Security > Speech Recognition.")
            return
        }

        let recognizer = SFSpeechRecognizer(locale: config.inputLocale) ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US"))
        guard let recognizer else {
            store.updateStatus("No compatible speech recognizer found for input locale. Falling back to en-US failed.")
            return
        }

        let transcriber = SpeechTranscriber(recognizer: recognizer, config: config)
        let store = store
        let translator = translator
        let outputMode = config.outputMode

        transcriber.onPartial = { text in
            Task { @MainActor in
                store.updatePartial(text)
                if (outputMode.showEnglish || outputMode.showChinese) && !outputMode.showOriginal {
                    translator.requestPartialTranslations(text: text)
                }
            }
        }

        transcriber.onFinal = { text in
            Task { @MainActor in
                let id = store.commitFinal(original: text)
                translator.clearPartialTranslations()
                translator.requestTranslations(id: id, text: text)
            }
        }

        transcriber.onError = { message in
            Task { @MainActor in
                store.updateStatus(message)
            }
        }

        self.transcriber = transcriber
        transcriber.start()

        do {
            try await startCapture()
            store.updateStatus("Listening...")
        } catch {
            store.updateStatus("Capture failed: \(error.localizedDescription)")
        }
    }

    @MainActor
    func stop() async {
        transcriber?.stop()
        transcriber = nil

        if let stream {
            do {
                try await stream.stopCapture()
            } catch {
                store.updateStatus("Failed to stop capture: \(error.localizedDescription)")
            }
        }
        stream = nil
    }

    @MainActor
    private func startCapture() async throws {
        let content = try await SCShareableContent.current
        guard let display = content.displays.first else {
            throw NSError(domain: "Subtitles", code: -1, userInfo: [NSLocalizedDescriptionKey: "No display found for capture."])
        }

        let filter = SCContentFilter(display: display, excludingWindows: [])
        let configuration = SCStreamConfiguration()
        configuration.capturesAudio = true
        configuration.sampleRate = config.captureSampleRate
        configuration.channelCount = config.captureChannelCount
        configuration.excludesCurrentProcessAudio = config.excludesCurrentProcessAudio

        let stream = SCStream(filter: filter, configuration: configuration, delegate: self)
        try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: audioQueue)
        try await stream.startCapture()
        self.stream = stream
    }

    @MainActor
    private func requestSpeechAuthorization() async -> Bool {
        let status = SFSpeechRecognizer.authorizationStatus()
        if status == .authorized {
            return true
        }
        return await withCheckedContinuation { continuation in
            DispatchQueue.main.async {
                SFSpeechRecognizer.requestAuthorization { status in
                    continuation.resume(returning: status == .authorized)
                }
            }
        }
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio else { return }
        guard let pcmBuffer = converter.convert(sampleBuffer: sampleBuffer) else { return }
        transcriber?.append(pcmBuffer)
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        let store = store
        Task { @MainActor in
            store.updateStatus("Capture stopped: \(error.localizedDescription)")
        }
    }
}
