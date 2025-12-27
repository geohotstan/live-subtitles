import Foundation

struct SubtitleLine: Identifiable, Equatable {
    let id: UUID
    let createdAt: Date
    var original: String
    var english: String?
    var chinese: String?
}

@MainActor
final class SubtitleStore: ObservableObject {
    @Published var lines: [SubtitleLine] = []
    @Published var partialOriginal: String = ""
    @Published var partialEnglish: String = ""
    @Published var partialChinese: String = ""
    @Published var statusMessage: String? = "Ready."

    private let maxHistory: Int

    init(maxHistory: Int) {
        self.maxHistory = max(1, maxHistory)
    }

    func updateStatus(_ message: String?) {
        statusMessage = message
    }

    func updatePartial(_ text: String) {
        partialOriginal = text
    }

    func updatePartialEnglish(_ text: String) {
        partialEnglish = text
    }

    func updatePartialChinese(_ text: String) {
        partialChinese = text
    }

    func commitFinal(original: String) -> UUID {
        let line = SubtitleLine(id: UUID(), createdAt: Date(), original: original, english: nil, chinese: nil)
        lines.append(line)
        if lines.count > maxHistory {
            lines.removeFirst(lines.count - maxHistory)
        }
        partialOriginal = ""
        partialEnglish = ""
        partialChinese = ""
        return line.id
    }

    func updateTranslation(id: UUID, english: String? = nil, chinese: String? = nil) {
        guard let index = lines.firstIndex(where: { $0.id == id }) else { return }
        if let english {
            lines[index].english = english
        }
        if let chinese {
            lines[index].chinese = chinese
        }
    }
}
