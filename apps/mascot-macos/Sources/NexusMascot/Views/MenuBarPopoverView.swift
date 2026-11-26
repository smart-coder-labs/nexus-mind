import SwiftUI

/// Menu Bar Popover featuring live Laya/Jev engine control, autonomy level,
/// NexusMind's signature Outcome Tape (last 14 runs), Privacy Shield & Kill Switch.
/// Uses strictly fixed sizing so macOS `MenuBarExtra(.window)` never jitters or clips.
struct MenuBarPopoverView: View {
    @ObservedObject var viewModel: MascotViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            // Header (static orb icon to keep MenuBarExtra layout 100% stable)
            HStack(spacing: 10) {
                SynapticOrbView(state: viewModel.visualState, size: 40, animated: false)
                VStack(alignment: .leading, spacing: 2) {
                    Text(viewModel.mascotName)
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(.white)
                    Text(viewModel.daemonConnected ? viewModel.visualState.label : "Daemon desconectado")
                        .font(.system(size: 12))
                        .foregroundStyle(.white.opacity(0.65))
                        .lineLimit(1)
                }
                Spacer(minLength: 8)
                Button {
                    Task { await viewModel.togglePrivacyShield() }
                } label: {
                    Text(viewModel.privacyShield ? "Privado ON" : "Privado")
                        .font(.system(size: 11, weight: .medium))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                        .background(viewModel.privacyShield ? NexusTokens.statusSlate.opacity(0.35) : Color.white.opacity(0.08))
                        .clipShape(Capsule())
                        .foregroundStyle(.white)
                }
                .buttonStyle(.plain)
            }

            Divider().overlay(NexusTokens.surfaceBorder)

            // Mandatory NexusMind Connection Status Banner
            if viewModel.nexusmindRequired && !viewModel.nexusmindConnected {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 6) {
                        Image(systemName: "lock.fill")
                            .foregroundStyle(NexusTokens.statusRose)
                        Text("NEXUSMIND REQUERIDO (BLOQUEADO)")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(NexusTokens.statusRose)
                    }
                    Text(viewModel.nexusmindError ?? "Conecta tu servidor NexusMind y API key para habilitar la mascota y su auditoría.")
                        .font(.system(size: 11))
                        .foregroundStyle(.white.opacity(0.8))
                    Button("Conectar NexusMind") {
                        SettingsWindowController.shared.open()
                    }
                    .buttonStyle(.plain)
                    .font(.system(size: 11, weight: .semibold))
                    .padding(.horizontal, 10)
                    .padding(.vertical, 5)
                    .background(NexusTokens.accentLime)
                    .foregroundStyle(NexusTokens.accentLimeForeground)
                    .clipShape(Capsule())
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(NexusTokens.statusRose.opacity(0.12))
                .clipShape(RoundedRectangle(cornerRadius: NexusTokens.radiusInput))
            } else {
                HStack(spacing: 6) {
                    Circle()
                        .fill(NexusTokens.statusSuccess)
                        .frame(width: 7, height: 7)
                    Text("NexusMind conectado · \(viewModel.deviceHostname)")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.white.opacity(0.75))
                        .lineLimit(1)
                }
            }

            // Decision Engine Switcher (Laya local vs Jev cloud vs Hybrid)
            VStack(alignment: .leading, spacing: 6) {
                Text("MOTOR DE DECISIONES (SYSTEM 1)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.white.opacity(0.55))
                Picker("Motor", selection: Binding(
                    get: { viewModel.decisionMode },
                    set: { mode in Task { await viewModel.updateDecisionMode(mode) } }
                )) {
                    ForEach(DecisionMode.allCases) { mode in
                        Text(mode.displayName).tag(mode)
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            // Autonomy Level
            VStack(alignment: .leading, spacing: 6) {
                Text("NIVEL DE AUTONOMÍA")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.white.opacity(0.55))
                Picker("Autonomía", selection: Binding(
                    get: { viewModel.autonomyLevel },
                    set: { level in Task { await viewModel.updateAutonomyLevel(level) } }
                )) {
                    ForEach(AutonomyLevel.allCases) { lvl in
                        Text(lvl.displayName).tag(lvl)
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            // Outcome Tape (signature NexusMind factory visual)
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text("OUTCOME TAPE (ÚLTIMAS 14 ACCIONES)")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.white.opacity(0.55))
                    Spacer()
                    Text("\(viewModel.outcomeTape.count)")
                        .font(.system(size: 11))
                        .foregroundStyle(.white.opacity(0.55))
                }
                HStack(spacing: 4) {
                    ForEach(0..<14, id: \.self) { idx in
                        let entry = idx < viewModel.outcomeTape.count ? viewModel.outcomeTape[idx] : nil
                        RoundedRectangle(cornerRadius: 3)
                            .fill(tapeColor(for: entry?.outcome))
                            .frame(width: 15, height: 15)
                            .help(entry?.summary ?? "Sin ejecución")
                    }
                }
            }

            // Live learning & capabilities counters
            HStack(spacing: 12) {
                Label("\(viewModel.learnedSkillsCount) skills", systemImage: "sparkles")
                Label("\(viewModel.mcpToolsCount) MCP", systemImage: "puzzlepiece.extension")
                if !viewModel.suggestions.isEmpty {
                    Label("\(viewModel.suggestions.count) ideas", systemImage: "lightbulb")
                        .foregroundStyle(NexusTokens.accentLime)
                }
            }
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(.white.opacity(0.75))

            // Quick open floating chat bar + voice input
            HStack(spacing: 8) {
                Button {
                    viewModel.openQuickPrompt()
                } label: {
                    HStack {
                        Image(systemName: "bubble.left.and.text.bubble.right")
                        Text("Escribir (⌘⇧N)")
                    }
                    .font(.system(size: 12, weight: .semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 7)
                    .background(NexusTokens.accentLime)
                    .foregroundStyle(NexusTokens.accentLimeForeground)
                    .clipShape(Capsule())
                }
                .buttonStyle(.plain)

                Button {
                    viewModel.toggleVoiceInput()
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: viewModel.isListeningVoice ? "stop.circle.fill" : "mic.fill")
                        Text(viewModel.isListeningVoice ? "Parar" : "Voz (⌘⇧V)")
                    }
                    .font(.system(size: 12, weight: .semibold))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 7)
                    .background(
                        viewModel.isListeningVoice
                            ? NexusTokens.statusRose.opacity(0.25)
                            : Color.white.opacity(0.08)
                    )
                    .foregroundStyle(viewModel.isListeningVoice ? NexusTokens.statusRose : .white)
                    .clipShape(Capsule())
                }
                .buttonStyle(.plain)
            }

            // Permissions checklist shortcut
            if !viewModel.permissions.allCoreGranted {
                VStack(alignment: .leading, spacing: 6) {
                    Text("PERMISOS MACOS PENDIENTES")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(NexusTokens.statusAmber)
                    HStack(spacing: 8) {
                        if !viewModel.permissions.accessibilityGranted {
                            Button("Activar Accesibilidad") {
                                MacOSPermissions.requestAccessibility()
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                        }
                        if !viewModel.permissions.screenRecordingGranted {
                            Button("Activar Pantalla") {
                                MacOSPermissions.openScreenRecordingSettings()
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                        }
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(NexusTokens.statusAmber.opacity(0.10))
                .clipShape(RoundedRectangle(cornerRadius: NexusTokens.radiusInput))
            }

            Divider().overlay(NexusTokens.surfaceBorder)

            // Settings + Emergency Kill Switch + Quit
            HStack(spacing: 8) {
                Button {
                    ActivityLogsWindowController.shared.open()
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: "list.bullet.rectangle")
                        Text("Logs")
                    }
                    .font(.system(size: 11, weight: .medium))
                    .padding(.horizontal, 9)
                    .padding(.vertical, 6)
                    .background(Color.white.opacity(0.08))
                    .foregroundStyle(.white)
                    .clipShape(Capsule())
                }
                .buttonStyle(.plain)

                Button {
                    SettingsWindowController.shared.open()
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: "key.fill")
                        Text("Keys")
                    }
                    .font(.system(size: 11, weight: .medium))
                    .padding(.horizontal, 9)
                    .padding(.vertical, 6)
                    .background(Color.white.opacity(0.08))
                    .foregroundStyle(.white)
                    .clipShape(Capsule())
                }
                .buttonStyle(.plain)

                Button {
                    Task { await viewModel.toggleKillSwitch() }
                } label: {
                    Text(viewModel.killSwitchActive ? "Reanudar" : "Pausar")
                        .font(.system(size: 11, weight: .semibold))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(viewModel.killSwitchActive ? NexusTokens.accentLime : NexusTokens.statusRose.opacity(0.22))
                        .foregroundStyle(viewModel.killSwitchActive ? NexusTokens.accentLimeForeground : NexusTokens.statusRose)
                        .clipShape(Capsule())
                }
                .buttonStyle(.plain)

                Spacer()

                Button("Salir") {
                    NSApplication.shared.terminate(nil)
                }
                .buttonStyle(.plain)
                .font(.system(size: 12))
                .foregroundStyle(.white.opacity(0.70))
            }
        }
        .padding(16)
        .frame(width: 320, alignment: .leading)
        .fixedSize(horizontal: true, vertical: true)
        .background(NexusTokens.bodyObsidian)
        .preferredColorScheme(.dark)
    }

    private func tapeColor(for outcome: String?) -> Color {
        switch outcome {
        case "success": return NexusTokens.statusSuccess
        case "held", "warning": return NexusTokens.statusAmber
        case "error": return NexusTokens.statusRose
        case "cancelled": return NexusTokens.statusSlate
        default: return Color.white.opacity(0.10)
        }
    }
}
