import SwiftUI
@preconcurrency import Translation
@preconcurrency import _Translation_SwiftUI

struct SubtitlesView: View {
    @ObservedObject var store: SubtitleStore
    let translator: TranslationBroker
    let config: AppConfig

    var body: some View {
        baseView
            .conditional(config.outputMode.showEnglish) { view in
                view.translationTask(source: config.translationSourceLanguage, target: config.englishTarget) { session in
                    await runTranslation(session: session, targetLabel: "English") { job in
                        let response = try await session.translate(job.text)
                        await MainActor.run {
                            translator.publishEnglish(id: job.id, text: response.targetText, isPartial: job.isPartial, sequence: job.sequence)
                        }
                    }
                }
            }
            .conditional(config.outputMode.showChinese) { view in
                view.translationTask(source: config.translationSourceLanguage, target: config.chineseTarget) { session in
                    await runTranslation(session: session, targetLabel: "Chinese") { job in
                        let response = try await session.translate(job.text)
                        await MainActor.run {
                            translator.publishChinese(id: job.id, text: response.targetText, isPartial: job.isPartial, sequence: job.sequence)
                        }
                    }
                }
            }
    }

    private var baseView: some View {
        ZStack(alignment: .bottom) {
            VStack(spacing: 10) {
                ForEach(store.lines) { line in
                    SubtitleCard(
                        original: line.original,
                        english: line.english,
                        chinese: line.chinese,
                        isPartial: false,
                        showOriginal: config.outputMode.showOriginal,
                        showEnglish: config.outputMode.showEnglish,
                        showChinese: config.outputMode.showChinese
                    )
                }

                let showPartialOriginal = config.outputMode.showOriginal && !store.partialOriginal.isEmpty
                let showPartialEnglish = config.outputMode.showEnglish && !store.partialEnglish.isEmpty
                let showPartialChinese = config.outputMode.showChinese && !store.partialChinese.isEmpty

                if showPartialOriginal || showPartialEnglish || showPartialChinese {
                    SubtitleCard(
                        original: store.partialOriginal,
                        english: store.partialEnglish,
                        chinese: store.partialChinese,
                        isPartial: true,
                        showOriginal: showPartialOriginal,
                        showEnglish: showPartialEnglish,
                        showChinese: showPartialChinese
                    )
                }

                if let status = store.statusMessage {
                    Text(status)
                        .font(.system(size: 12, weight: .medium, design: .rounded))
                        .foregroundStyle(Color.white.opacity(0.7))
                        .padding(.top, 6)
                }
                if store.lines.isEmpty && store.partialOriginal.isEmpty && store.statusMessage == nil {
                    Text("Subtitles running…")
                        .font(.system(size: 14, weight: .semibold, design: .rounded))
                        .foregroundStyle(Color.white.opacity(0.85))
                }
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 28)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(
            LinearGradient(
                colors: [
                    Color.black.opacity(config.debugOverlay ? 0.15 : 0.0),
                    Color.black.opacity(config.debugOverlay ? 0.45 : 0.25)
                ],
                startPoint: .top,
                endPoint: .bottom
            )
        )
        .animation(.easeOut(duration: 0.15), value: store.lines)
    }

    private func runTranslation(
        session: TranslationSession,
        targetLabel: String,
        translate: @escaping (TranslationJob) async throws -> Void
    ) async {
        do {
            try await session.prepareTranslation()
        } catch {
            await MainActor.run {
                translator.publishError("\(targetLabel) translation not ready: \(error.localizedDescription)")
            }
        }

        let stream = await MainActor.run { targetLabel == "Chinese" ? translator.chineseJobs() : translator.englishJobs() }
        for await job in stream {
            do {
                try await translate(job)
            } catch {
                await MainActor.run {
                    translator.publishError("\(targetLabel) translation error: \(error.localizedDescription)")
                }
            }
        }
    }
}

private struct SubtitleCard: View {
    let original: String
    let english: String?
    let chinese: String?
    let isPartial: Bool
    let showOriginal: Bool
    let showEnglish: Bool
    let showChinese: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if showOriginal {
                Text(original)
                    .font(.system(size: 28, weight: .semibold, design: .rounded))
                    .foregroundStyle(Color.white)
            }

            if showEnglish {
                let text = english ?? ((isPartial && !showOriginal) || !isPartial ? "…" : nil)
                if let text {
                    Text(text)
                        .font(.system(size: 20, weight: .medium, design: .rounded))
                        .foregroundStyle(Color.white.opacity(0.9))
                }
            }

            if showChinese {
                let text = chinese ?? ((isPartial && !showOriginal) || !isPartial ? "…" : nil)
                if let text {
                    Text(text)
                        .font(.system(size: 20, weight: .medium, design: .rounded))
                        .foregroundStyle(Color.white.opacity(0.9))
                }
            }
        }
        .padding(.vertical, 14)
        .padding(.horizontal, 18)
        .frame(maxWidth: 920, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color.black.opacity(isPartial ? 0.45 : 0.65))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(Color.white.opacity(0.08), lineWidth: 1)
        )
    }
}

private extension View {
    @ViewBuilder
    func conditional<Content: View>(_ condition: Bool, transform: (Self) -> Content) -> some View {
        if condition {
            transform(self)
        } else {
            self
        }
    }
}
