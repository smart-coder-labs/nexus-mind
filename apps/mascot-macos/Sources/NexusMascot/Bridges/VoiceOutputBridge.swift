import AVFoundation
import Foundation

/// Native macOS text-to-speech bridge using `AVSpeechSynthesizer`.
/// Automatically prefers Apple's highest-quality local neural voice (Premium > Enhanced > Default)
/// for the user's locale and cleans Markdown/URLs before speaking.
@MainActor
final class VoiceOutputBridge: NSObject, AVSpeechSynthesizerDelegate {
    private let synthesizer = AVSpeechSynthesizer()
    var onSpeakingChanged: (@MainActor (Bool) -> Void)?

    override init() {
        super.init()
        synthesizer.delegate = self
    }

    var isSpeaking: Bool {
        synthesizer.isSpeaking
    }

    func speak(_ rawText: String, languageHint: String? = nil) {
        let cleaned = Self.cleanForSpeech(rawText)
        guard !cleaned.isEmpty else { return }

        if synthesizer.isSpeaking {
            synthesizer.stopSpeaking(at: .immediate)
        }

        let utterance = AVSpeechUtterance(string: cleaned)
        utterance.voice = Self.bestVoice(languageHint: languageHint)
        utterance.rate = AVSpeechUtteranceDefaultSpeechRate * 1.03
        utterance.pitchMultiplier = 1.02
        utterance.preUtteranceDelay = 0.05

        onSpeakingChanged?(true)
        synthesizer.speak(utterance)
    }

    func stop() {
        if synthesizer.isSpeaking {
            synthesizer.stopSpeaking(at: .immediate)
        }
        onSpeakingChanged?(false)
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        Task { @MainActor in
            self.onSpeakingChanged?(false)
        }
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        Task { @MainActor in
            self.onSpeakingChanged?(false)
        }
    }

    /// Selects the highest-quality local macOS voice matching the target language (prefers Spanish/system locale).
    private static func bestVoice(languageHint: String?) -> AVSpeechSynthesisVoice? {
        let preferredLang = languageHint ?? Locale.current.identifier.replacingOccurrences(of: "_", with: "-")
        let langPrefix = String(preferredLang.prefix(2)).lowercased()
        let voices = AVSpeechSynthesisVoice.speechVoices()

        let matching = voices.filter { $0.language.lowercased().hasPrefix(langPrefix) }
        let spanishFallback = voices.filter { $0.language.lowercased().hasPrefix("es") }
        let pool = !matching.isEmpty ? matching : (!spanishFallback.isEmpty ? spanishFallback : voices)

        if #available(macOS 13.0, *) {
            if let premium = pool.first(where: { $0.quality == .premium }) {
                return premium
            }
        }
        if let enhanced = pool.first(where: { $0.quality == .enhanced }) {
            return enhanced
        }
        return pool.first ?? AVSpeechSynthesisVoice(language: "es-ES")
    }

    /// Strips Markdown syntax, fenced code blocks, and raw URLs so spoken responses sound natural.
    static func cleanForSpeech(_ raw: String) -> String {
        var text = raw
        // Replace fenced code blocks with a brief spoken note
        text = text.replacingOccurrences(
            of: "```[\\s\\S]*?```",
            with: " (bloque de código en pantalla) ",
            options: .regularExpression
        )
        // Markdown links [label](url) -> label
        text = text.replacingOccurrences(
            of: "\\[([^\\]]+)\\]\\([^\\)]+\\)",
            with: "$1",
            options: .regularExpression
        )
        // Raw URLs
        text = text.replacingOccurrences(
            of: "https?://\\S+",
            with: "enlace en pantalla",
            options: .regularExpression
        )
        // Markdown emphasis, headings, inline backticks, bullets
        for token in ["**", "__", "`", "### ", "## ", "# ", "- "] {
            text = text.replacingOccurrences(of: token, with: "")
        }
        // Collapse excessive whitespace
        text = text.replacingOccurrences(
            of: "\\s+",
            with: " ",
            options: .regularExpression
        )
        .trimmingCharacters(in: .whitespacesAndNewlines)

        // Keep spoken output concise if the LLM wrote a long essay
        if text.count > 650 {
            let idx = text.index(text.startIndex, offsetBy: 650)
            text = String(text[..<idx]) + "… Puedes leer el resto en pantalla."
        }
        return text
    }
}
