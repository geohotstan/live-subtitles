import Foundation
@preconcurrency import Speech

final class SpeechTranscriber {
    private let recognizer: SFSpeechRecognizer
    private let config: AppConfig
    private let queue = DispatchQueue(label: "subtitles.speech.queue", qos: .userInitiated)

    var onPartial: (@Sendable (String) -> Void)?
    var onFinal: (@Sendable (String) -> Void)?
    var onError: (@Sendable (String) -> Void)?

    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var isRunning = false
    private var lastPartialUpdate = Date.distantPast
    private var lastPartialText: String = ""

    init(recognizer: SFSpeechRecognizer, config: AppConfig) {
        self.recognizer = recognizer
        self.config = config
    }

    func start() {
        queue.async { [weak self] in
            self?.startLocked()
        }
    }

    func append(_ buffer: AVAudioPCMBuffer) {
        queue.async { [weak self] in
            self?.request?.append(buffer)
        }
    }

    func stop() {
        queue.async { [weak self] in
            self?.request?.endAudio()
            self?.task?.cancel()
            self?.request = nil
            self?.task = nil
            self?.isRunning = false
        }
    }

    private func startLocked() {
        guard !isRunning else { return }
        isRunning = true

        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.addsPunctuation = true
        request.taskHint = .dictation
        if config.preferOnDeviceRecognition && recognizer.supportsOnDeviceRecognition {
            request.requiresOnDeviceRecognition = true
        }

        self.request = request
        self.task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            guard let self else { return }
            if let result {
                self.handle(result: result)
            }
            if let error {
                self.handle(error: error)
            }
        }
    }

    private func handle(result: SFSpeechRecognitionResult) {
        let text = result.bestTranscription.formattedString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }

        if result.isFinal {
            lastPartialText = ""
            lastPartialUpdate = Date.distantPast
            onFinal?(text)
            return
        }

        let now = Date()
        guard now.timeIntervalSince(lastPartialUpdate) >= config.partialDebounceSeconds else { return }
        guard text != lastPartialText else { return }

        lastPartialUpdate = now
        lastPartialText = text
        onPartial?(text)
    }

    private func handle(error: Error) {
        onError?("Speech error: \(error.localizedDescription). Restarting...")

        queue.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            self?.request?.endAudio()
            self?.task?.cancel()
            self?.request = nil
            self?.task = nil
            self?.isRunning = false
            self?.startLocked()
        }
    }
}
