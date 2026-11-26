import AppKit
import SwiftUI

/// Manages the native macOS Settings window for API keys, LLM provider, Laya/Jev & NexusMind sync.
@MainActor
final class SettingsWindowController {
    static let shared = SettingsWindowController()
    private var window: NSWindow?

    func open() {
        if let window {
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }

        let hosting = NSHostingController(rootView: SettingsWindowView())
        let win = NSWindow(contentViewController: hosting)
        win.title = "Configuración — NexusMind Mascot"
        win.styleMask = [.titled, .closable, .miniaturizable]
        win.isReleasedWhenClosed = false
        win.setContentSize(NSSize(width: 480, height: 610))
        win.center()
        win.backgroundColor = NSColor(red: 0.11, green: 0.11, blue: 0.11, alpha: 1.0)
        self.window = win

        NSApp.activate(ignoringOtherApps: true)
        win.makeKeyAndOrderFront(nil)
    }
}

struct SettingsWindowView: View {
    private let client = MascotDaemonClient()

    @State private var mascotName = "Nexi"
    @State private var llmProvider: LlmProviderKind = .claude
    @State private var llmModel = "claude-sonnet-4-5"
    @State private var decisionMode: DecisionMode = .hybrid
    @State private var layaEndpoint = "http://127.0.0.1:8787/v1/systemone"
    @State private var nexusmindEnabled = true
    @State private var nexusmindBaseUrl = "http://localhost:8080"
    @State private var nexusmindProject = "personal-mascot"

    // Secret inputs (never prefilled with raw keys; only sent if non-empty)
    @State private var anthropicKey = ""
    @State private var geminiKey = ""
    @State private var openAiKey = ""
    @State private var jevKey = ""
    @State private var nexusmindKey = ""

    @State private var hasAnthropicKey = false
    @State private var hasGeminiKey = false
    @State private var hasOpenAiKey = false
    @State private var hasJevKey = false
    @State private var hasNexusmindKey = false

    @State private var statusBanner: String?
    @State private var isError = false
    @State private var isSaving = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                headerSection
                system2Section
                system1Section
                nexusMindSection

                if let banner = statusBanner {
                    Text(banner)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(isError ? NexusTokens.statusRose : NexusTokens.statusSuccess)
                        .padding(10)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background((isError ? NexusTokens.statusRose : NexusTokens.statusSuccess).opacity(0.12))
                        .clipShape(RoundedRectangle(cornerRadius: NexusTokens.radiusInput))
                }

                HStack {
                    Text("Las keys se guardan en ~/.nexusmind/secrets.env (modo 0600)")
                        .font(.system(size: 11))
                        .foregroundStyle(.white.opacity(0.5))
                    Spacer()
                    Button(isSaving ? "Guardando…" : "Guardar y aplicar") {
                        Task { await save() }
                    }
                    .buttonStyle(.plain)
                    .disabled(isSaving)
                    .font(.system(size: 13, weight: .semibold))
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .background(NexusTokens.accentLime)
                    .foregroundStyle(NexusTokens.accentLimeForeground)
                    .clipShape(Capsule())
                }
            }
            .padding(20)
        }
        .frame(width: 480, height: 610)
        .background(NexusTokens.bodyObsidian)
        .preferredColorScheme(.dark)
        .task {
            await load()
        }
    }

    private var headerSection: some View {
        HStack(spacing: 12) {
            SynapticOrbView(state: .idle, size: 42, animated: false)
            VStack(alignment: .leading, spacing: 2) {
                Text("Configuración de Mascota")
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(.white)
                Text("Conecta tus API keys (Claude, Gemini, OpenAI, Jev, NexusMind) y aplica en caliente.")
                    .font(.system(size: 12))
                    .foregroundStyle(.white.opacity(0.65))
            }
        }
    }

    private var system2Section: some View {
        sectionCard("CEREBRO SYSTEM 2 (LLM Y HERRAMIENTAS)") {
            fieldRow("Nombre de la mascota") {
                TextField("Nexi", text: $mascotName)
                    .textFieldStyle(.roundedBorder)
            }
            fieldRow("Proveedor principal") {
                Picker("", selection: $llmProvider) {
                    ForEach(LlmProviderKind.allCases) { p in
                        Text(p.displayName).tag(p)
                    }
                }
                .labelsHidden()
                .onChange(of: llmProvider) { _, next in
                    llmModel = next.defaultModel
                }
            }
            fieldRow("Modelo") {
                TextField("claude-sonnet-4-5", text: $llmModel)
                    .textFieldStyle(.roundedBorder)
            }
            secretRow("Claude API Key (ANTHROPIC_API_KEY)", configured: hasAnthropicKey, text: $anthropicKey, placeholder: "sk-ant-…")
            secretRow("Gemini API Key (GEMINI_API_KEY)", configured: hasGeminiKey, text: $geminiKey, placeholder: "AIza…")
            secretRow("OpenAI API Key (OPENAI_API_KEY)", configured: hasOpenAiKey, text: $openAiKey, placeholder: "sk-…")
        }
    }

    private var system1Section: some View {
        sectionCard("MOTOR DE DECISIONES SYSTEM 1 (LAYA LOCAL / JEV CLOUD)") {
            fieldRow("Modo") {
                Picker("", selection: $decisionMode) {
                    ForEach(DecisionMode.allCases) { m in
                        Text(m.displayName).tag(m)
                    }
                }
                .labelsHidden()
            }
            fieldRow("Endpoint Laya local") {
                TextField("http://127.0.0.1:8787/v1/systemone", text: $layaEndpoint)
                    .textFieldStyle(.roundedBorder)
            }
            secretRow("Jev / TypeSafe API Key (TYPESAFE_API_KEY)", configured: hasJevKey, text: $jevKey, placeholder: "ts_…")
        }
    }

    private var nexusMindSection: some View {
        sectionCard("MEMORIA COMPARTIDA NEXUSMIND (MULTI-PC)") {
            Toggle("Sincronizar aprendizajes y skills con NexusMind", isOn: $nexusmindEnabled)
                .font(.system(size: 12))
                .foregroundStyle(.white)
            fieldRow("URL del servidor NexusMind") {
                TextField("http://localhost:8080", text: $nexusmindBaseUrl)
                    .textFieldStyle(.roundedBorder)
            }
            fieldRow("Proyecto") {
                TextField("personal-mascot", text: $nexusmindProject)
                    .textFieldStyle(.roundedBorder)
            }
            secretRow("NexusMind API Key (NEXUSMIND_API_KEY)", configured: hasNexusmindKey, text: $nexusmindKey, placeholder: "nm_…")
        }
    }

    private func sectionCard<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.white.opacity(0.55))
            content()
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(NexusTokens.surfaceElevated)
        .clipShape(RoundedRectangle(cornerRadius: NexusTokens.radiusCard))
        .overlay(
            RoundedRectangle(cornerRadius: NexusTokens.radiusCard)
                .stroke(NexusTokens.surfaceBorder, lineWidth: 1)
        )
    }

    private func fieldRow<Content: View>(_ label: String, @ViewBuilder field: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.white.opacity(0.82))
            field()
        }
    }

    private func secretRow(_ label: String, configured: Bool, text: Binding<String>, placeholder: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(label)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.white.opacity(0.82))
                Spacer()
                Text(configured ? "Configurada" : "Sin configurar")
                    .font(.system(size: 10, weight: .semibold))
                    .padding(.horizontal, 7)
                    .padding(.vertical, 2)
                    .background((configured ? NexusTokens.statusSuccess : NexusTokens.statusSlate).opacity(0.16))
                    .foregroundStyle(configured ? NexusTokens.statusSuccess : NexusTokens.statusSlate)
                    .clipShape(Capsule())
            }
            SecureField(configured ? "•••••••• (dejar vacío para mantener actual)" : placeholder, text: text)
                .textFieldStyle(.roundedBorder)
        }
    }

    private func load() async {
        do {
            let s = try await client.getSettings()
            mascotName = s.mascotName
            decisionMode = s.decisionMode
            layaEndpoint = s.layaEndpoint
            llmProvider = s.llmProvider
            llmModel = s.llmModel
            nexusmindEnabled = s.nexusmindEnabled
            nexusmindBaseUrl = s.nexusmindBaseUrl
            nexusmindProject = s.nexusmindProject
            hasAnthropicKey = s.hasAnthropicKey
            hasGeminiKey = s.hasGeminiKey
            hasOpenAiKey = s.hasOpenAiKey
            hasJevKey = s.hasJevKey
            hasNexusmindKey = s.hasNexusmindKey
            statusBanner = nil
        } catch {
            isError = true
            statusBanner = "No se pudo conectar al daemon local (~/.nexusmind/mascot.sock). Asegúrate de que nexus-mascot-daemon esté corriendo."
        }
    }

    private func save() async {
        isSaving = true
        defer { isSaving = false }
        var params: [String: Any] = [
            "mascot_name": mascotName,
            "decision_mode": decisionMode.rawValue,
            "laya_endpoint": layaEndpoint,
            "llm_provider": llmProvider.rawValue,
            "llm_model": llmModel,
            "nexusmind_enabled": nexusmindEnabled,
            "nexusmind_base_url": nexusmindBaseUrl,
            "nexusmind_project": nexusmindProject
        ]
        if !anthropicKey.trimmingCharacters(in: .whitespaces).isEmpty { params["anthropic_api_key"] = anthropicKey }
        if !geminiKey.trimmingCharacters(in: .whitespaces).isEmpty { params["gemini_api_key"] = geminiKey }
        if !openAiKey.trimmingCharacters(in: .whitespaces).isEmpty { params["openai_api_key"] = openAiKey }
        if !jevKey.trimmingCharacters(in: .whitespaces).isEmpty { params["jev_api_key"] = jevKey }
        if !nexusmindKey.trimmingCharacters(in: .whitespaces).isEmpty { params["nexusmind_api_key"] = nexusmindKey }

        do {
            let updated = try await client.saveSettings(params: params)
            hasAnthropicKey = updated.hasAnthropicKey
            hasGeminiKey = updated.hasGeminiKey
            hasOpenAiKey = updated.hasOpenAiKey
            hasJevKey = updated.hasJevKey
            hasNexusmindKey = updated.hasNexusmindKey
            anthropicKey = ""
            geminiKey = ""
            openAiKey = ""
            jevKey = ""
            nexusmindKey = ""
            isError = false
            statusBanner = "Configuración y API keys guardadas y aplicadas en caliente."
            await MascotViewModel.shared.refresh()
        } catch {
            isError = true
            statusBanner = "Error al guardar: \(error.localizedDescription)"
        }
    }
}
