import AppKit
import SwiftUI

/// Native macOS Audit Window: full conversation transcripts + live activity/decision logs.
@MainActor
final class ActivityLogsWindowController {
    static let shared = ActivityLogsWindowController()
    private var window: NSWindow?

    func open() {
        if let window {
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }
        let hosting = NSHostingController(rootView: ActivityLogsWindowView())
        let win = NSWindow(contentViewController: hosting)
        win.title = "Auditoría, Logs y Conversaciones — NexusMind Mascot"
        win.styleMask = [.titled, .closable, .miniaturizable, .resizable]
        win.isReleasedWhenClosed = false
        win.setContentSize(NSSize(width: 720, height: 560))
        win.center()
        win.backgroundColor = NSColor(red: 0.11, green: 0.11, blue: 0.11, alpha: 1.0)
        self.window = win
        NSApp.activate(ignoringOtherApps: true)
        win.makeKeyAndOrderFront(nil)
    }
}

struct ActivityLogsWindowView: View {
    private let client = MascotDaemonClient()
    @State private var tab: Int = 0
    @State private var conversations: [ConversationRecord] = []
    @State private var logs: [ActivityLogEntry] = []
    @State private var selectedConversationId: String?
    @State private var kindFilter: String = "all"
    @State private var errorMsg: String?
    @State private var syncBanner: String?
    @State private var isSyncing: Bool = false

    var body: some View {
        VStack(spacing: 0) {
            // Top bar
            HStack(spacing: 12) {
                Picker("", selection: $tab) {
                    Text("Logs de Auditoría (\(logs.count))").tag(0)
                    Text("Conversaciones (\(conversations.count))").tag(1)
                }
                .pickerStyle(.segmented)
                .frame(width: 360)

                Spacer()

                if tab == 0 {
                    Picker("Filtro", selection: $kindFilter) {
                        Text("Todos").tag("all")
                        Text("prompt").tag("prompt")
                        Text("reply").tag("reply")
                        Text("action").tag("action")
                        Text("approval").tag("approval")
                        Text("kill_switch").tag("kill_switch")
                    }
                    .frame(width: 130)
                    .onChange(of: kindFilter) { _, _ in
                        Task { await reload() }
                    }
                }

                Button(isSyncing ? "Migrando…" : "Migrar a NexusMind") {
                    Task { await runMigration() }
                }
                .buttonStyle(.borderedProminent)
                .tint(NexusTokens.accentLime)
                .foregroundStyle(NexusTokens.accentLimeForeground)
                .disabled(isSyncing)

                Button {
                    Task { await reload() }
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.bordered)
            }
            .padding(14)
            .background(NexusTokens.surfaceElevated)

            Divider().overlay(NexusTokens.surfaceBorder)

            if let banner = syncBanner {
                Text(banner)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(NexusTokens.statusSuccess)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(NexusTokens.statusSuccess.opacity(0.12))
            }

            if let err = errorMsg {
                Text(err)
                    .font(.system(size: 12))
                    .foregroundStyle(NexusTokens.statusRose)
                    .padding(12)
            }

            if tab == 0 {
                logsListView
            } else {
                conversationsView
            }
        }
        .frame(minWidth: 640, minHeight: 480)
        .background(NexusTokens.bodyObsidian)
        .preferredColorScheme(.dark)
        .task {
            await reload()
        }
    }

    private var logsListView: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 8) {
                if logs.isEmpty {
                    Text("Sin eventos registrados todavía.")
                        .font(.system(size: 13))
                        .foregroundStyle(.white.opacity(0.5))
                        .padding(24)
                } else {
                    ForEach(logs) { log in
                        VStack(alignment: .leading, spacing: 5) {
                            HStack(spacing: 8) {
                                Text(log.eventKind.uppercased())
                                    .font(.system(size: 10, weight: .bold))
                                    .padding(.horizontal, 7)
                                    .padding(.vertical, 2)
                                    .background(badgeColor(log.outcome).opacity(0.18))
                                    .foregroundStyle(badgeColor(log.outcome))
                                    .clipShape(Capsule())

                                Text(log.summary)
                                    .font(.system(size: 13, weight: .medium))
                                    .foregroundStyle(.white)
                                    .lineLimit(2)

                                Spacer()

                                Text(shortTime(log.timestamp))
                                    .font(.system(size: 11, design: .monospaced))
                                    .foregroundStyle(.white.opacity(0.45))
                            }

                            HStack(spacing: 10) {
                                if let src = log.decisionSource {
                                    Text("Motor: \(src)")
                                }
                                if let risk = log.risk {
                                    Text(String(format: "Riesgo: %.0f%%", risk * 100))
                                }
                                if let out = log.outcome {
                                    Text("Estado: \(out)")
                                }
                            }
                            .font(.system(size: 11))
                            .foregroundStyle(.white.opacity(0.6))

                            if !log.detailJson.isEmpty && log.detailJson != "{}" {
                                DisclosureGroup("Ver detalle JSON") {
                                    Text(log.detailJson)
                                        .font(.system(size: 11, design: .monospaced))
                                        .foregroundStyle(.white.opacity(0.8))
                                        .textSelection(.enabled)
                                        .padding(8)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                        .background(Color.black.opacity(0.35))
                                        .clipShape(RoundedRectangle(cornerRadius: 8))
                                }
                                .font(.system(size: 11))
                                .foregroundStyle(.white.opacity(0.65))
                            }
                        }
                        .padding(12)
                        .background(NexusTokens.surfaceElevated)
                        .clipShape(RoundedRectangle(cornerRadius: 12))
                        .overlay(
                            RoundedRectangle(cornerRadius: 12)
                                .stroke(NexusTokens.surfaceBorder, lineWidth: 1)
                        )
                    }
                }
            }
            .padding(14)
        }
    }

    private var conversationsView: some View {
        HSplitView {
            List(conversations, selection: $selectedConversationId) { conv in
                VStack(alignment: .leading, spacing: 4) {
                    Text(conv.objective)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(.white)
                        .lineLimit(2)
                    HStack {
                        Text(conv.status)
                            .font(.system(size: 10, weight: .semibold))
                            .foregroundStyle(conv.status == "completed" ? NexusTokens.statusSuccess : NexusTokens.statusAmber)
                        Spacer()
                        Text("\(conv.stepsExecuted) pasos")
                            .font(.system(size: 11))
                            .foregroundStyle(.white.opacity(0.55))
                    }
                }
                .padding(.vertical, 4)
                .tag(conv.id)
            }
            .frame(minWidth: 230, maxWidth: 280)

            ScrollView {
                if let selected = conversations.first(where: { $0.id == selectedConversationId }) ?? conversations.first {
                    VStack(alignment: .leading, spacing: 12) {
                        Text(selected.objective)
                            .font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(.white)

                        HStack(spacing: 12) {
                            Text("Estado: \(selected.status)")
                            Text("Pasos: \(selected.stepsExecuted)")
                            if let route = selected.route { Text("Ruta: \(route)") }
                            Text(shortTime(selected.updatedAt))
                        }
                        .font(.system(size: 11))
                        .foregroundStyle(.white.opacity(0.6))

                        if let reply = selected.finalReply, !reply.isEmpty {
                            VStack(alignment: .leading, spacing: 4) {
                                Text("RESPUESTA FINAL")
                                    .font(.system(size: 10, weight: .bold))
                                    .foregroundStyle(NexusTokens.accentLime)
                                Text(reply)
                                    .font(.system(size: 13))
                                    .foregroundStyle(.white)
                                    .textSelection(.enabled)
                            }
                            .padding(12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(NexusTokens.surfaceElevated)
                            .clipShape(RoundedRectangle(cornerRadius: 12))
                        }

                        VStack(alignment: .leading, spacing: 4) {
                            Text("TRANSCRIPCIÓN Y HERRAMIENTAS (RAW JSON)")
                                .font(.system(size: 10, weight: .bold))
                                .foregroundStyle(.white.opacity(0.55))
                            Text(selected.stateJson)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundStyle(.white.opacity(0.8))
                                .textSelection(.enabled)
                                .padding(10)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(Color.black.opacity(0.35))
                                .clipShape(RoundedRectangle(cornerRadius: 10))
                        }
                    }
                    .padding(16)
                } else {
                    Text("Sin conversaciones registradas.")
                        .font(.system(size: 13))
                        .foregroundStyle(.white.opacity(0.5))
                        .padding(24)
                }
            }
        }
    }

    private func runMigration() async {
        isSyncing = true
        syncBanner = nil
        errorMsg = nil
        defer { isSyncing = false }
        do {
            let r = try await client.syncHistoryToNexusMind()
            syncBanner = "Migrado a NexusMind: \(r.conversations) conversaciones, \(r.events) logs/eventos y \(r.skills) skills."
            await reload()
        } catch {
            errorMsg = "Error migrando a NexusMind: \(error.localizedDescription)"
        }
    }

    private func reload() async {
        do {
            logs = try await client.listActivityLogs(eventKind: kindFilter, limit: 150)
            conversations = try await client.listConversations(limit: 60)
            if selectedConversationId == nil {
                selectedConversationId = conversations.first?.id
            }
            errorMsg = nil
        } catch {
            errorMsg = "Error cargando logs: \(error.localizedDescription)"
        }
    }

    private func badgeColor(_ outcome: String?) -> Color {
        switch outcome {
        case "success": return NexusTokens.statusSuccess
        case "held", "warning": return NexusTokens.statusAmber
        case "error", "blocked": return NexusTokens.statusRose
        default: return NexusTokens.statusBlue
        }
    }

    private func shortTime(_ iso: String) -> String {
        if iso.count >= 19 {
            return String(iso.prefix(19)).replacingOccurrences(of: "T", with: " ")
        }
        return iso
    }
}
