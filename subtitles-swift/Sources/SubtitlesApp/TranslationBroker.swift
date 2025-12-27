import Foundation

struct TranslationJob: Sendable {
    let id: UUID?
    let text: String
    let isPartial: Bool
    let sequence: Int
}

@MainActor
final class TranslationBroker {
    private let store: SubtitleStore
    private var latestPartialSequence: Int = 0

    private let englishStream: AsyncStream<TranslationJob>
    private let chineseStream: AsyncStream<TranslationJob>
    private var englishContinuation: AsyncStream<TranslationJob>.Continuation
    private var chineseContinuation: AsyncStream<TranslationJob>.Continuation

    init(store: SubtitleStore) {
        self.store = store
        var englishCont: AsyncStream<TranslationJob>.Continuation!
        self.englishStream = AsyncStream { continuation in
            englishCont = continuation
        }
        self.englishContinuation = englishCont

        var chineseCont: AsyncStream<TranslationJob>.Continuation!
        self.chineseStream = AsyncStream { continuation in
            chineseCont = continuation
        }
        self.chineseContinuation = chineseCont
    }

    func requestTranslations(id: UUID, text: String) {
        let job = TranslationJob(id: id, text: text, isPartial: false, sequence: 0)
        englishContinuation.yield(job)
        chineseContinuation.yield(job)
    }

    func requestPartialTranslations(text: String) {
        latestPartialSequence += 1
        let job = TranslationJob(id: nil, text: text, isPartial: true, sequence: latestPartialSequence)
        englishContinuation.yield(job)
        chineseContinuation.yield(job)
    }

    func englishJobs() -> AsyncStream<TranslationJob> {
        englishStream
    }

    func chineseJobs() -> AsyncStream<TranslationJob> {
        chineseStream
    }

    func publishEnglish(id: UUID?, text: String, isPartial: Bool, sequence: Int) {
        if isPartial || id == nil {
            guard sequence == latestPartialSequence else { return }
            store.updatePartialEnglish(text)
        } else if let id {
            store.updateTranslation(id: id, english: text)
        }
    }

    func publishChinese(id: UUID?, text: String, isPartial: Bool, sequence: Int) {
        if isPartial || id == nil {
            guard sequence == latestPartialSequence else { return }
            store.updatePartialChinese(text)
        } else if let id {
            store.updateTranslation(id: id, chinese: text)
        }
    }

    func clearPartialTranslations() {
        latestPartialSequence += 1
        store.updatePartialEnglish("")
        store.updatePartialChinese("")
    }

    func publishError(_ message: String) {
        store.updateStatus(message)
    }
}
