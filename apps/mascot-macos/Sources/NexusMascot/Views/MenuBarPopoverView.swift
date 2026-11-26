import SwiftUI

/// Menu Bar Popover featuring live Laya/Jev engine control, autonomy level,
/// NexusMind's signature Outcome Tape (last 14 runs), Privacy Shield & Kill Switch.
struct MenuBarPopoverView: View {
    @ObservedObject var viewModel: MascotViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            // Header
            HStack(spacing: 10) {
                SynapticOrbView(state: viewModel.visualState, size: 42)
                VStack(alignment: .leading, spacing: 2) {
                    Text(viewModel.mascotName)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(.white)
                    Text(viewModel.daemonConnected ? viewModel.visualState.label : "Daemon desconectado")
                        .font(.system(size: 12))
                        .foregroundStyle(.white.opacity(0.65))
                }
                Spacer()
                Button(viewModel.privacyShield ? "Privado ON" : "Privado") {
                    Task { await viewModel.togglePrivacyShield() }
                }
                .buttonStyle(.plain)
                .font(.system(size: 11, weight: .medium))
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(viewModel.privacyShield ? NexusTokens.statusSlate.opacity(0.3) : Color.white.opacity(0.06))
                .clipShape(Capsule())
                .foregroundStyle(.white)
            }

            Divider().overlay(NexusTokens.surfaceBorder)

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
                            .frame(width: 14, height: 14)
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
            .foregroundStyle(.white.opacity(0.72))

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
                            .font(.system(size: 11))
                        }
                        if !viewModel.permissions.screenRecordingGranted {
                            Button("Activar Pantalla") {
                                MacOSPermissions.openScreenRecordingSettings()
                            }
                            .font(.system(size: 11))
                        }
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(NexusTokens.statusAmber.opacity(0.10))
                .clipShape(RoundedRectangle(cornerRadius: NexusTokens.radiusInput))
            }

            Divider().overlay(NexusTokens.surfaceBorder)

            // Emergency Kill Switch + Quit
            HStack {
                Button(viewModel.killSwitchActive ? "Reanudar mascota" : "Pausar todo (Kill Switch)") {
                    Task { await viewModel.toggleKillSwitch() }
                }
                .buttonStyle(.plain)
                .font(.system(size: 12, weight: .semibold))
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(viewModel.killSwitchActive ? NexusTokens.accentLime : NexusTokens.statusRose.opacity(0.22))
                .foregroundStyle(viewModel.killSwitchActive ? NexusTokens.accentLimeForeground : NexusTokens.statusRose)
                .clipShape(Capsule())

                Spacer()

                Button("Salir") {
                    NSApplication.shared.terminate(nil)
                }
                .buttonStyle(.plain)
                .font(.system(size: 12))
                .foregroundStyle(.white.opacity(0.65))
            }
        }
        .padding(16)
        .frame(width: 310)
        .background(NexusTokens.bodyObsidian)
    }

    private func tapeColor(for outcome: String?) -> Color {
        switch outcome {
        case "success": return NexusTokens.statusSuccess
        case "held", "warning": return NexusTokens.statusAmber
        case "error": return NexusTokens.statusRose
        case "cancelled": return NexusTokens.statusSlate
        default: return Color.white.opacity(0.08)
        }
    }
}
