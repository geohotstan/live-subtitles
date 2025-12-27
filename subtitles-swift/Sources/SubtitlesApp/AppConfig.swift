import AVFoundation
import Foundation

struct AppConfig {
    let inputLocale: Locale
    let translationSourceLanguage: Locale.Language?
    let englishTarget: Locale.Language
    let chineseTarget: Locale.Language
    let outputMode: OutputMode
    let maxHistory: Int
    let partialDebounceSeconds: TimeInterval
    let excludesCurrentProcessAudio: Bool
    let captureSampleRate: Int
    let captureChannelCount: Int
    let outputSampleRate: Double
    let outputChannels: AVAudioChannelCount
    let audioGain: Float
    let preferOnDeviceRecognition: Bool
    let debugOverlay: Bool

    static func load() -> AppConfig {
        let args = CommandLine.arguments
        let env = ProcessInfo.processInfo.environment

        func argValue(_ name: String) -> String? {
            guard let idx = args.firstIndex(of: name), idx + 1 < args.count else { return nil }
            return args[idx + 1]
        }

        func flag(_ name: String) -> Bool {
            args.contains(name)
        }

        let inputLanguage = argValue("--input-language") ?? env["SUBTITLES_INPUT_LANGUAGE"]
        let inputLocaleArg = argValue("--input-locale") ?? env["SUBTITLES_INPUT_LOCALE"]
        let hasExplicitInput = inputLanguage != nil || inputLocaleArg != nil
        let inputLocaleId = inputLocaleArg
            ?? mapLanguageToLocale(inputLanguage)
            ?? Locale.current.identifier

        let maxHistory = Int(argValue("--max-history") ?? "2") ?? 2
        let partialDebounceMs = Double(argValue("--partial-debounce-ms") ?? "200") ?? 200
        let outputMode = OutputMode.parse(argValue("--output-mode") ?? env["SUBTITLES_OUTPUT_MODE"])
        let outputSampleRate = Double(argValue("--output-sample-rate") ?? env["SUBTITLES_OUTPUT_SAMPLE_RATE"] ?? "16000") ?? 16000
        let outputChannelsValue = Int(argValue("--output-channels") ?? env["SUBTITLES_OUTPUT_CHANNELS"] ?? "1") ?? 1
        let audioGain = Float(argValue("--audio-gain") ?? env["SUBTITLES_AUDIO_GAIN"] ?? "1.0") ?? 1.0

        let excludeSelfAudio = !flag("--include-self-audio")
        let preferOnDevice = !flag("--allow-cloud-recognition")
        let debugOverlay = flag("--debug-overlay") || (env["SUBTITLES_DEBUG_OVERLAY"] == "1")

        let inputLocale = Locale(identifier: inputLocaleId)
        let translationSourceLanguage = hasExplicitInput
            ? Locale.Language(identifier: mapLanguageToLanguageIdentifier(inputLanguage) ?? inputLocale.languageCode ?? inputLocaleId)
            : nil

        return AppConfig(
            inputLocale: inputLocale,
            translationSourceLanguage: translationSourceLanguage,
            englishTarget: Locale.Language(identifier: "en"),
            chineseTarget: Locale.Language(identifier: "zh-Hans"),
            outputMode: outputMode,
            maxHistory: max(1, maxHistory),
            partialDebounceSeconds: max(0.05, partialDebounceMs / 1000.0),
            excludesCurrentProcessAudio: excludeSelfAudio,
            captureSampleRate: 48_000,
            captureChannelCount: 2,
            outputSampleRate: outputSampleRate,
            outputChannels: AVAudioChannelCount(max(1, outputChannelsValue)),
            audioGain: max(0.1, min(4.0, audioGain)),
            preferOnDeviceRecognition: preferOnDevice,
            debugOverlay: debugOverlay
        )
    }
}

private func mapLanguageToLocale(_ language: String?) -> String? {
    guard let language else { return nil }
    switch language.lowercased() {
    case "ja", "jp", "japanese":
        return "ja-JP"
    case "en", "english":
        return "en-US"
    case "zh", "cn", "chinese", "zh-cn", "zh-hans":
        return "zh-CN"
    default:
        return nil
    }
}

private func mapLanguageToLanguageIdentifier(_ language: String?) -> String? {
    guard let language else { return nil }
    switch language.lowercased() {
    case "ja", "jp", "japanese":
        return "ja"
    case "en", "english":
        return "en"
    case "zh", "cn", "chinese", "zh-cn", "zh-hans":
        return "zh-Hans"
    default:
        return nil
    }
}

enum OutputMode: String {
    case bilingual
    case english
    case chinese
    case englishChinese
    case original

    var showOriginal: Bool {
        switch self {
        case .bilingual, .original:
            return true
        case .english, .chinese, .englishChinese:
            return false
        }
    }

    var showEnglish: Bool {
        switch self {
        case .bilingual, .english, .englishChinese:
            return true
        case .chinese, .original:
            return false
        }
    }

    var showChinese: Bool {
        switch self {
        case .bilingual, .chinese, .englishChinese:
            return true
        case .english, .original:
            return false
        }
    }

    static func parse(_ value: String?) -> OutputMode {
        guard let value = value?.lowercased() else { return .bilingual }
        switch value {
        case "english", "en":
            return .english
        case "chinese", "zh", "cn":
            return .chinese
        case "english-chinese", "en-zh", "bilingual-translation":
            return .englishChinese
        case "original", "source":
            return .original
        case "bilingual", "all":
            return .bilingual
        default:
            return .bilingual
        }
    }
}
