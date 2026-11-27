import AppKit
import SwiftUI

@MainActor
final class MascotViewModel: ObservableObject {
    static let shared = MascotViewModel()

    @Published var mascotName: String = "Nexi"
    @Published var visualState: MascotVisualState = .idle
    @Published var decisionMode: DecisionMode = .hybrid
    @Published var autonomyLevel: AutonomyLevel = .supervisedAutonomous
    @Published var privacyShield: Bool = false
    @Published var killSwitchActive: Bool = false
    @Published var daemonConnected: Bool = false
    @Published var nexusmindRequired: Bool = true
    @Published var nexusmindConnected: Bool = false
    @Published var nexusmindError: String?
    @Published var deviceHostname: String = ""
    @Published var pendingApprovals: [PendingApproval] = []
    @Published var outcomeTape: [OutcomeEntry] = []
    @Published var suggestions: [ProactiveSuggestion] = []
    @Published var learnedSkillsCount: Int = 0
    @Published var mcpToolsCount: Int = 0
    @Published var permissions: PermissionsSnapshot = MacOSPermissions.snapshot()

    @Published var showQuickPrompt: Bool = false
    @Published var promptDraft: String = ""
    @Published var lastReply: String?
    @Published var isListeningVoice: Bool = false
    @Published var isSpeakingVoice: Bool = false
    @Published var voiceReplyEnabled: Bool = {
        if UserDefaults.standard.object(forKey: "nexi_voice_reply_enabled") == nil {
            return true
        }
        return UserDefaults.standard.bool(forKey: "nexi_voice_reply_enabled")
    }() {
        didSet {
            UserDefaults.standard.set(voiceReplyEnabled, forKey: "nexi_voice_reply_enabled")
            if !voiceReplyEnabled {
                speaker.stop()
            }
        }
    }
    @Published var voiceError: String?

    weak var floatingPanel: FloatingCompanionPanel?

    private let client = MascotDaemonClient()
    private let observer = ActiveContextObserver()
    private let voice = VoiceInputBridge()
    private let speaker = VoiceOutputBridge()
    private var statusPollTask: Task<Void, Never>?
    private var osJobPollTask: Task<Void, Never>?
    private var lastActiveApp: String?
    private var started = false

    func start() {
        guard !started else { return }
        started = true

        speaker.onSpeakingChanged = { [weak self] speaking in
            self?.isSpeakingVoice = speaking
        }

        // Register global system-wide hotkeys (⌘⇧N, ⌘⇧V, ⌘⇧P, ⌘⇧⎋)
        GlobalHotkeys.shared.onQuickCommand = { [weak self] in
            Task { @MainActor in self?.toggleQuickPrompt() }
        }
        GlobalHotkeys.shared.onVoiceToggle = { [weak self] in
            Task { @MainActor in self?.toggleVoiceInput() }
        }
        GlobalHotkeys.shared.onPrivacyShield = { [weak self] in
            Task { @MainActor in await self?.togglePrivacyShield() }
        }
        GlobalHotkeys.shared.onKillSwitch = { [weak self] in
            Task { @MainActor in await self?.triggerKillSwitch() }
        }
        GlobalHotkeys.shared.registerAll()

        observer.start { [weak self] appName in
            Task { @MainActor in
                guard let self else { return }
                self.lastActiveApp = appName
                guard !self.privacyShield else { return }
                let windowTitle = AccessibilityBridge.frontmostWindowTitle() ?? appName
                try? await self.client.recordObservation(
                    source: "active_window",
                    appName: appName,
                    summary: "Switched to \(appName): \(windowTitle)"
                )
            }
        }

        // Poll daemon status calmly (only updates @Published when values actually change)
        statusPollTask = Task {
            while !Task.isCancelled {
                await refresh()
                try? await Task.sleep(for: .seconds(2))
            }
        }

        // Fast background OS-bridge worker (never triggers UI re-renders unless needed)
        osJobPollTask = Task {
            while !Task.isCancelled {
                await processPendingOsJobs()
                try? await Task.sleep(for: .milliseconds(750))
            }
        }
    }

    func toggleQuickPrompt() {
        showQuickPrompt.toggle()
        if showQuickPrompt {
            floatingPanel?.focusForTyping()
        }
    }

    func openQuickPrompt() {
        showQuickPrompt = true
        floatingPanel?.focusForTyping()
    }

    func toggleVoiceInput() {
        speaker.stop()
        openQuickPrompt()
        voiceError = nil
        voice.toggleRecording(
            onPartialTranscript: { [weak self] text in
                guard let self else { return }
                self.isListeningVoice = true
                self.visualState = .listening
                self.promptDraft = text
                self.voiceError = self.voice.lastError
            },
            onFinalStop: { [weak self] finalText in
                guard let self else { return }
                self.isListeningVoice = false
                self.promptDraft = finalText
                Task { await self.sendPrompt() }
            }
        )
        // Reflect immediate state transition after async permission check
        Task {
            try? await Task.sleep(for: .milliseconds(250))
            self.isListeningVoice = self.voice.isRecording
            self.voiceError = self.voice.lastError
            if self.voice.isRecording {
                self.visualState = .listening
            } else if self.visualState == .listening {
                self.visualState = .idle
            }
        }
    }

    func refresh() async {
        let newPerms = MacOSPermissions.snapshot()
        if newPerms.accessibilityGranted != permissions.accessibilityGranted
            || newPerms.screenRecordingGranted != permissions.screenRecordingGranted {
            permissions = newPerms
        }

        do {
            let status = try await client.getStatus()
            if !daemonConnected { daemonConnected = true }
            if mascotName != status.mascotName { mascotName = status.mascotName }
            if !isListeningVoice && visualState != status.visualState {
                visualState = status.visualState
            }
            if decisionMode != status.decisionMode { decisionMode = status.decisionMode }
            if autonomyLevel != status.autonomyLevel { autonomyLevel = status.autonomyLevel }
            if privacyShield != status.privacyShield { privacyShield = status.privacyShield }
            if killSwitchActive != status.killSwitch { killSwitchActive = status.killSwitch }
            let nmReq = status.nexusmindRequired ?? true
            if nexusmindRequired != nmReq { nexusmindRequired = nmReq }
            let nmConn = status.nexusmindConnected ?? false
            if nexusmindConnected != nmConn { nexusmindConnected = nmConn }
            if nexusmindError != status.nexusmindError { nexusmindError = status.nexusmindError }
            let host = status.hostname ?? status.deviceId
            if deviceHostname != host { deviceHostname = host }
            let skillsCount = status.learnedSkills ?? 0
            if learnedSkillsCount != skillsCount { learnedSkillsCount = skillsCount }
            let mcpCount = status.mcpTools ?? 0
            if mcpToolsCount != mcpCount { mcpToolsCount = mcpCount }

            let newApprovals = (try? await client.listPendingApprovals()) ?? []
            if newApprovals.map(\.id) != pendingApprovals.map(\.id) {
                let prevIds = Set(pendingApprovals.map(\.id))
                pendingApprovals = newApprovals
                if let newest = newApprovals.first(where: { !prevIds.contains($0.id) }) {
                    speakIfAllowed("Necesito tu aprobación para: \(newest.summary)")
                }
            }
            let newTape = (try? await client.getOutcomeTape(limit: 14)) ?? []
            if newTape.map(\.id) != outcomeTape.map(\.id) {
                outcomeTape = newTape
            }
            let newSuggestions = (try? await client.listSuggestions()) ?? []
            if newSuggestions.map(\.id) != suggestions.map(\.id) {
                suggestions = newSuggestions
            }
        } catch {
            if daemonConnected { daemonConnected = false }
        }
    }

    private func processPendingOsJobs() async {
        guard daemonConnected, !killSwitchActive else { return }
        guard let jobs = try? await client.pollOsJobs(), !jobs.isEmpty else { return }

        for job in jobs {
            let payload = (try? JSONSerialization.jsonObject(with: Data(job.payloadJson.utf8)) as? [String: Any]) ?? [:]
            do {
                switch job.kind {
                case "ax_snapshot":
                    let tree = AccessibilityBridge.captureSemanticTree(maxNodes: 80)
                    try await client.completeOsJob(id: job.id, ok: true, result: ["nodes": tree])
                case "ax_click":
                    let label = payload["label"] as? String ?? ""
                    let ok = AccessibilityBridge.pressElement(matching: label)
                    try await client.completeOsJob(id: job.id, ok: ok, result: ["pressed": ok, "label": label])
                case "screenshot":
                    let b64 = try await ScreenCaptureBridge.captureMainDisplayBase64PNG()
                    try await client.completeOsJob(id: job.id, ok: true, result: ["png_base64": b64])
                case "mouse_click":
                    let x = payload["x"] as? Double ?? 0
                    let y = payload["y"] as? Double ?? 0
                    EventActuator.leftClick(at: CGPoint(x: x, y: y))
                    try await client.completeOsJob(id: job.id, ok: true, result: ["clicked": [x, y]])
                case "type_text":
                    let text = payload["text"] as? String ?? ""
                    EventActuator.typeText(text)
                    try await client.completeOsJob(id: job.id, ok: true, result: ["typed_chars": text.count])
                default:
                    try await client.completeOsJob(id: job.id, ok: false, result: ["error": "unknown job kind"])
                }
            } catch {
                try? await client.completeOsJob(id: job.id, ok: false, result: ["error": error.localizedDescription])
            }
        }
    }

    func toggleVoiceReply() {
        if isSpeakingVoice {
            speaker.stop()
        } else {
            voiceReplyEnabled.toggle()
        }
    }

    func stopSpeaking() {
        speaker.stop()
    }

    func speakIfAllowed(_ text: String) {
        guard voiceReplyEnabled, !privacyShield, !killSwitchActive else { return }
        speaker.speak(text)
    }

    func togglePrivacyShield() async {
        let next = !privacyShield
        if next { speaker.stop() }
        try? await client.setPrivacyShield(enabled: next)
        await refresh()
    }

    func updateDecisionMode(_ mode: DecisionMode) async {
        try? await client.setDecisionMode(mode)
        await refresh()
    }

    func updateAutonomyLevel(_ level: AutonomyLevel) async {
        try? await client.setAutonomyLevel(level)
        await refresh()
    }

    func triggerKillSwitch() async {
        speaker.stop()
        try? await client.setKillSwitch(active: true)
        await refresh()
    }

    func toggleKillSwitch() async {
        if !killSwitchActive { speaker.stop() }
        try? await client.setKillSwitch(active: !killSwitchActive)
        await refresh()
    }

    func resolveApproval(id: String, approved: Bool) async {
        try? await client.resolveApproval(id: id, approved: approved)
        await refresh()
    }

    func sendPrompt() async {
        let text = promptDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        promptDraft = ""
        speaker.stop()
        visualState = .thinking
        do {
            let reply = try await client.submitPrompt(text, activeApp: lastActiveApp)
            lastReply = reply
            speakIfAllowed(reply)
        } catch {
            let errText = "Error: \(error.localizedDescription)"
            lastReply = errText
            speakIfAllowed(error.localizedDescription)
        }
        await refresh()
    }

    func handleDroppedItems(_ providers: [NSItemProvider]) {
        openQuickPrompt()
        for provider in providers {
            if provider.canLoadObject(ofClass: URL.self) {
                _ = provider.loadObject(ofClass: URL.self) { [weak self] url, _ in
                    guard let url else { return }
                    Task { @MainActor in
                        self?.promptDraft = "Analiza este archivo: \(url.path)"
                    }
                }
            }
        }
    }
}

final class MascotAppDelegate: NSObject, NSApplicationDelegate {
    var panel: FloatingCompanionPanel?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Essential when running via `swift run` (without .app bundle) so MenuBarExtra & NSPanel behave cleanly
        NSApp.setActivationPolicy(.accessory)

        Task { @MainActor in
            let vm = MascotViewModel.shared
            vm.start()
            let p = FloatingCompanionPanel(rootView: FloatingCompanionRootView(viewModel: vm))
            vm.floatingPanel = p
            p.orderFrontRegardless()
            self.panel = p
        }
    }
}

@main
struct NexusMascotApp: App {
    @NSApplicationDelegateAdaptor(MascotAppDelegate.self) private var appDelegate
    @ObservedObject private var viewModel = MascotViewModel.shared

    var body: some Scene {
        MenuBarExtra {
            MenuBarPopoverView(viewModel: viewModel)
        } label: {
            Image(systemName: viewModel.privacyShield ? "eye.slash.circle" : "circle.hexagongrid.fill")
                .symbolRenderingMode(.hierarchical)
        }
        .menuBarExtraStyle(.window)
    }
}
