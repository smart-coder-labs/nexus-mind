import AVFoundation
import Foundation
import Speech

/// Native macOS microphone + on-device speech recognition bridge (`AVAudioEngine` + `SFSpeechRecognizer`).
/// Prefers on-device recognition when supported by macOS for maximum privacy and low latency.
@MainActor
final class VoiceInputBridge: ObservableObject {
    @Published private(set) var isRecording: Bool = false
    @Published private(set) var lastError: String?

    private let audioEngine = AVAudioEngine()
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var recognitionTask: SFSpeechRecognitionTask?
    private var speechRecognizer: SFSpeechRecognizer?

    func toggleRecording(
        onPartialTranscript: @escaping @MainActor (String) -> Void,
        onFinalStop: @escaping @MainActor (String) -> Void
    ) {
        if isRecording {
            stopRecording(onFinalStop: onFinalStop)
        } else {
            Task {
                await startRecording(
                    onPartialTranscript: onPartialTranscript,
                    onFinalStop: onFinalStop
                )
            }
        }
    }

    private var latestTranscript: String = ""

    private func requestPermissions() async -> Bool {
        // 1. Microphone permission
        let micGranted = await withCheckedContinuation { continuation in
            switch AVCaptureDevice.authorizationStatus(for: .audio) {
            case .authorized:
                continuation.resume(returning: true)
            case .notDetermined:
                AVCaptureDevice.requestAccess(for: .audio) { granted in
                    continuation.resume(returning: granted)
                }
            default:
                continuation.resume(returning: false)
            }
        }
        guard micGranted else {
            lastError = "Permiso de micrófono denegado. Actívalo en Ajustes del Sistema → Privacidad → Micrófono."
            return false
        }

        // 2. Speech recognition permission
        let speechStatus = await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { status in
                continuation.resume(returning: status)
            }
        }
        guard speechStatus == .authorized else {
            lastError = "Permiso de reconocimiento de voz denegado. Actívalo en Privacidad → Reconocimiento de voz."
            return false
        }

        return true
    }

    private func startRecording(
        onPartialTranscript: @escaping @MainActor (String) -> Void,
        onFinalStop: @escaping @MainActor (String) -> Void
    ) async {
        lastError = nil
        latestTranscript = ""

        guard await requestPermissions() else { return }

        let recognizer = SFSpeechRecognizer(locale: Locale.current)
            ?? SFSpeechRecognizer(locale: Locale(identifier: "es-ES"))
            ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US"))

        guard let recognizer, recognizer.isAvailable else {
            lastError = "El motor de reconocimiento de voz de macOS no está disponible ahora."
            return
        }
        self.speechRecognizer = recognizer

        cleanupEngine()

        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        if recognizer.supportsOnDeviceRecognition {
            request.requiresOnDeviceRecognition = true
        }
        self.recognitionRequest = request

        let inputNode = audioEngine.inputNode
        let format = inputNode.outputFormat(forBus: 0)
        inputNode.removeTap(onBus: 0)
        inputNode.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
            request.append(buffer)
        }

        audioEngine.prepare()
        do {
            try audioEngine.start()
        } catch {
            lastError = "No se pudo iniciar el micrófono: \(error.localizedDescription)"
            cleanupEngine()
            return
        }

        isRecording = true

        recognitionTask = recognizer.recognitionTask(with: request) { [weak self] result, error in
            Task { @MainActor in
                guard let self else { return }
                if let result {
                    let text = result.bestTranscription.formattedString
                    self.latestTranscript = text
                    onPartialTranscript(text)
                    if result.isFinal {
                        self.stopRecording(onFinalStop: onFinalStop)
                    }
                }
                if let error, self.isRecording {
                    // Normal cancellation fires an error when user taps stop; only surface real errors
                    let nsErr = error as NSError
                    if nsErr.code != 216 && nsErr.code != 209 {
                        self.lastError = "Error de dictado: \(error.localizedDescription)"
                    }
                    self.cleanupEngine()
                    self.isRecording = false
                }
            }
        }
    }

    func stopRecording(onFinalStop: @escaping @MainActor (String) -> Void) {
        guard isRecording else { return }
        let captured = latestTranscript.trimmingCharacters(in: .whitespacesAndNewlines)
        cleanupEngine()
        isRecording = false
        if !captured.isEmpty {
            onFinalStop(captured)
        }
    }

    private func cleanupEngine() {
        if audioEngine.isRunning {
            audioEngine.stop()
        }
        audioEngine.inputNode.removeTap(onBus: 0)
        recognitionRequest?.endAudio()
        recognitionRequest = nil
        recognitionTask?.cancel()
        recognitionTask = nil
    }
}
