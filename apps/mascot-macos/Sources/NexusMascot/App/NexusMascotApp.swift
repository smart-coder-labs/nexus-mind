import AppKit
import SwiftUI

@MainActor
final class MascotViewModel: ObservableObject {
    @Published var mascotName: String = "Nexi"
    @Published var visualState: MascotVisualState = .idle
    @Published var decisionMode: DecisionMode = .hybrid
    @Published var autonomyLevel: AutonomyLevel = .supervisedAutonomous
    @Published var privacyShield: Bool = false
    @Published var killSwitchActive: Bool = false
    @Published var daemonConnected: Bool = false
    @Published var pendingApprovals: [PendingApproval] = []
    @Published var outcomeTape: [OutcomeEntry] = []
    @Published var suggestions: [ProactiveSuggestion] = []
    @Published var learnedSkillsCount: Int = 0
    @Published var mcpToolsCount: Int = 0
    @Published var permissions: PermissionsSnapshot = MacOSPermissions.snapshot()

    @Published var showQuickPrompt: Bool = false
    @Published var promptDraft: String = ""
    @Published var lastReply: String?

    private let client = MascotDaemonClient()
    private let observer = ActiveContextObserver()
    private var pollTask: Task<Void, Never>?
    private var lastActiveApp: String?

    func start() {
        // Register global system-wide hotkeys (⌘⇧N, ⌘⇧P, ⌘⇧⎋)
        GlobalHotkeys.shared.onQuickCommand = { [weak self] in
            Task { @MainActor in self?.showQuickPrompt.toggle() }
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

        pollTask = Task {
            while !Task.isCancelled {
                await refresh()
                await processPendingOsJobs()
                try? await Task.sleep(for: .milliseconds(750))
            }
        }
    }

    func refresh() async {
        permissions = MacOSPermissions.snapshot()
        do {
            let status = try await client.getStatus()
            daemonConnected = true
            mascotName = status.mascotName
            visualState = status.visualState
            decisionMode = status.decisionMode
            autonomyLevel = status.autonomyLevel
            privacyShield = status.privacyShield
            killSwitchActive = status.killSwitch
            learnedSkillsCount = status.learnedSkills ?? 0
            mcpToolsCount = status.mcpTools ?? 0
            pendingApprovals = (try? await client.listPendingApprovals()) ?? []
            outcomeTape = (try? await client.getOutcomeTape(limit: 14)) ?? []
            suggestions = (try? await client.listSuggestions()) ?? []
        } catch {
            daemonConnected = false
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

    func togglePrivacyShield() async {
        let next = !privacyShield
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
        try? await client.setKillSwitch(active: true)
        await refresh()
    }

    func toggleKillSwitch() async {
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
        visualState = .thinking
        do {
            lastReply = try await client.submitPrompt(text, activeApp: lastActiveApp)
        } catch {
            lastReply = "Error: \(error.localizedDescription)"
        }
        await refresh()
    }

    func handleDroppedItems(_ providers: [NSItemProvider]) {
        showQuickPrompt = true
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

@main
struct NexusMascotApp: App {
    @StateObject private var viewModel = MascotViewModel()
    @State private var panel: FloatingCompanionPanel?

    var body: some Scene {
        MenuBarExtra {
            MenuBarPopoverView(viewModel: viewModel)
                .task {
                    if panel == nil {
                        viewModel.start()
                        let p = FloatingCompanionPanel(
                            rootView: FloatingCompanionRootView(viewModel: viewModel)
                        )
                        p.orderFrontRegardless()
                        panel = p
                    }
                }
        } label: {
            Image(systemName: viewModel.privacyShield ? "eye.slash.circle" : "circle.hexagongrid.fill")
                .symbolRenderingMode(.hierarchical)
        }
        .menuBarExtraStyle(.window)
    }
}
