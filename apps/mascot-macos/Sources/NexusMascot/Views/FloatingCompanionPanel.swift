import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Transparent, always-on-top, draggable floating desktop companion panel.
/// Overrides `canBecomeKey` so TextFields inside QuickCommandBar accept keyboard input cleanly.
final class FloatingCompanionPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    init<Content: View>(rootView: Content) {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 340, height: 320),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        isFloatingPanel = true
        level = .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        isMovableByWindowBackground = true
        hidesOnDeactivate = false

        let hosting = NSHostingView(rootView: rootView)
        hosting.wantsLayer = true
        hosting.layer?.backgroundColor = NSColor.clear.cgColor
        contentView = hosting

        if let screen = NSScreen.main {
            let visible = screen.visibleFrame
            setFrameOrigin(NSPoint(x: visible.maxX - 360, y: visible.minY + 32))
        }
    }

    func focusForTyping() {
        NSApp.activate(ignoringOtherApps: true)
        makeKeyAndOrderFront(nil)
    }
}

struct FloatingCompanionRootView: View {
    @ObservedObject var viewModel: MascotViewModel
    @State private var isDropTargeted = false

    var body: some View {
        VStack(alignment: .trailing, spacing: 10) {
            Spacer(minLength: 0)

            if let approval = viewModel.pendingApprovals.first {
                ApprovalPillView(approval: approval) { approved in
                    Task { await viewModel.resolveApproval(id: approval.id, approved: approved) }
                }
            }

            if viewModel.showQuickPrompt {
                QuickCommandBarView(viewModel: viewModel)
            }

            HStack {
                Spacer()
                SynapticOrbView(state: viewModel.visualState, size: 78, animated: true)
                    .overlay(
                        Circle()
                            .strokeBorder(
                                NexusTokens.accentLime,
                                lineWidth: isDropTargeted ? 2 : 0
                            )
                    )
                    .onTapGesture {
                        viewModel.toggleQuickPrompt()
                    }
                    .onDrop(of: [.fileURL, .plainText], isTargeted: $isDropTargeted) { providers in
                        viewModel.handleDroppedItems(providers)
                        return true
                    }
                    .contextMenu {
                        Button(viewModel.isListeningVoice ? "Detener micrófono (⌘⇧V)" : "Hablar por micrófono (⌘⇧V)") {
                            viewModel.toggleVoiceInput()
                        }
                        Button(viewModel.showQuickPrompt ? "Ocultar chat" : "Abrir chat (⌘⇧N)") {
                            viewModel.toggleQuickPrompt()
                        }
                        Button("Configuración y API Keys…") {
                            SettingsWindowController.shared.open()
                        }
                        Button(viewModel.privacyShield ? "Desactivar Modo Privado" : "Activar Modo Privado (⌘⇧P)") {
                            Task { await viewModel.togglePrivacyShield() }
                        }
                        Button("Kill Switch de Emergencia (⌘⇧⎋)", role: .destructive) {
                            Task { await viewModel.triggerKillSwitch() }
                        }
                    }
            }
        }
        .padding(8)
        .frame(width: 340, height: 320, alignment: .bottomTrailing)
        .background(Color.clear)
        .preferredColorScheme(.dark)
    }
}

/// "Held means held" — plain-language approval card with 1-click decision.
struct ApprovalPillView: View {
    let approval: PendingApproval
    let onDecide: (Bool) -> Void
    @State private var showCommand = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Circle()
                    .fill(NexusTokens.statusAmber)
                    .frame(width: 8, height: 8)
                Text("Acción retenida para tu decisión")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(NexusTokens.statusAmber)
                Spacer()
                Text(String(format: "Riesgo %.0f%%", approval.risk * 100))
                    .font(.system(size: 11, weight: .medium))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 2)
                    .background(NexusTokens.statusAmber.opacity(0.14))
                    .clipShape(Capsule())
                    .foregroundStyle(NexusTokens.statusAmber)
            }

            Text(approval.summary)
                .font(.system(size: 13))
                .foregroundStyle(.white)
                .fixedSize(horizontal: false, vertical: true)

            Text(approval.reason)
                .font(.system(size: 11))
                .foregroundStyle(.white.opacity(0.68))

            if let cmd = approval.commandPreview, !cmd.isEmpty {
                DisclosureGroup("Ver comando", isExpanded: $showCommand) {
                    Text(cmd)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.white.opacity(0.85))
                        .padding(6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.black.opacity(0.35))
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                }
                .font(.system(size: 11))
                .foregroundStyle(.white.opacity(0.75))
            }

            HStack(spacing: 8) {
                Button("Permitir una vez") {
                    onDecide(true)
                }
                .buttonStyle(.plain)
                .font(.system(size: 12, weight: .semibold))
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(NexusTokens.accentLime)
                .foregroundStyle(NexusTokens.accentLimeForeground)
                .clipShape(Capsule())

                Button("Denegar") {
                    onDecide(false)
                }
                .buttonStyle(.plain)
                .font(.system(size: 12, weight: .medium))
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .overlay(Capsule().stroke(NexusTokens.surfaceBorder, lineWidth: 1))
                .foregroundStyle(.white.opacity(0.85))
            }
        }
        .padding(14)
        .frame(width: 310)
        .background(NexusTokens.bodyObsidian)
        .clipShape(RoundedRectangle(cornerRadius: NexusTokens.radiusCard))
        .overlay(
            RoundedRectangle(cornerRadius: NexusTokens.radiusCard)
                .stroke(NexusTokens.statusAmber.opacity(0.45), lineWidth: 1)
        )
    }
}

struct QuickCommandBarView: View {
    @ObservedObject var viewModel: MascotViewModel
    @FocusState private var inputFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Button {
                    viewModel.toggleVoiceInput()
                } label: {
                    Image(systemName: viewModel.isListeningVoice ? "stop.circle.fill" : "mic.fill")
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(viewModel.isListeningVoice ? NexusTokens.statusRose : NexusTokens.accentLime)
                        .frame(width: 26, height: 26)
                        .background(
                            (viewModel.isListeningVoice ? NexusTokens.statusRose : NexusTokens.accentLime)
                                .opacity(0.16)
                        )
                        .clipShape(Circle())
                }
                .buttonStyle(.plain)
                .help(viewModel.isListeningVoice ? "Detener dictado y enviar (⌘⇧V)" : "Hablar por micrófono (⌘⇧V)")

                TextField(
                    viewModel.isListeningVoice ? "Escuchando… habla ahora" : "Pídele algo a \(viewModel.mascotName)…",
                    text: $viewModel.promptDraft
                )
                    .textFieldStyle(.plain)
                    .font(.system(size: 13))
                    .foregroundStyle(.white)
                    .focused($inputFocused)
                    .onSubmit {
                        Task { await viewModel.sendPrompt() }
                    }

                Button("Enviar") {
                    Task { await viewModel.sendPrompt() }
                }
                .buttonStyle(.plain)
                .font(.system(size: 12, weight: .semibold))
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(NexusTokens.accentLime)
                .foregroundStyle(NexusTokens.accentLimeForeground)
                .clipShape(Capsule())
            }
            .padding(10)
            .background(NexusTokens.surfaceElevated)
            .clipShape(RoundedRectangle(cornerRadius: NexusTokens.radiusInput))
            .overlay(
                RoundedRectangle(cornerRadius: NexusTokens.radiusInput)
                    .stroke(
                        viewModel.isListeningVoice ? NexusTokens.accentLime.opacity(0.75) : NexusTokens.surfaceBorder,
                        lineWidth: 1
                    )
            )

            if let voiceErr = viewModel.voiceError {
                Text(voiceErr)
                    .font(.system(size: 11))
                    .foregroundStyle(NexusTokens.statusRose)
                    .padding(.horizontal, 4)
            }

            if let reply = viewModel.lastReply {
                Text(reply)
                    .font(.system(size: 12))
                    .foregroundStyle(.white.opacity(0.88))
                    .lineLimit(5)
                    .padding(.horizontal, 4)
            }
        }
        .padding(12)
        .frame(width: 310)
        .background(NexusTokens.bodyObsidian)
        .clipShape(RoundedRectangle(cornerRadius: NexusTokens.radiusCard))
        .overlay(
            RoundedRectangle(cornerRadius: NexusTokens.radiusCard)
                .stroke(NexusTokens.surfaceBorder, lineWidth: 1)
        )
        .onAppear {
            inputFocused = true
        }
    }
}
