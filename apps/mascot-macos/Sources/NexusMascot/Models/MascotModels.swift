import Foundation

enum MascotVisualState: String, Codable, CaseIterable {
    case idle
    case listening
    case deciding
    case thinking
    case acting
    case held
    case learning
    case privacyShield = "privacy_shield"
    case blocked

    var label: String {
        switch self {
        case .idle: return "En reposo"
        case .listening: return "Escuchando"
        case .deciding: return "Evaluando riesgo (System 1)"
        case .thinking: return "Pensando (System 2)"
        case .acting: return "Ejecutando acción"
        case .held: return "Esperando tu aprobación"
        case .learning: return "Aprendiendo y guardando"
        case .privacyShield: return "Modo privado"
        case .blocked: return "Bloqueado / Kill Switch"
        }
    }
}

enum DecisionMode: String, Codable, CaseIterable, Identifiable {
    case hybrid
    case layaLocal = "laya_local"
    case jevCloud = "jev_cloud"
    case deterministicOnly = "deterministic_only"

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .hybrid: return "Híbrido (Laya local → Jev)"
        case .layaLocal: return "Laya Local (Offline)"
        case .jevCloud: return "Jev Cloud (TypeSafe)"
        case .deterministicOnly: return "Solo reglas locales"
        }
    }
}

enum AutonomyLevel: String, Codable, CaseIterable, Identifiable {
    case observer
    case copilot
    case supervisedAutonomous = "supervised_autonomous"
    case fullAutonomous = "full_autonomous"

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .observer: return "0 · Observador"
        case .copilot: return "1 · Copiloto"
        case .supervisedAutonomous: return "2 · Autónomo supervisado"
        case .fullAutonomous: return "3 · Autónomo completo"
        }
    }
}

struct DaemonStatus: Codable {
    let mascotName: String
    let deviceId: String
    let visualState: MascotVisualState
    let decisionMode: DecisionMode
    let layaEndpoint: String
    let autonomyLevel: AutonomyLevel
    let privacyShield: Bool
    let killSwitch: Bool
    let llmProvider: String
    let llmConfigured: Bool
    let nexusmindConfigured: Bool
    let nexusmindRequired: Bool?
    let nexusmindConnected: Bool?
    let nexusmindError: String?
    let hostname: String?
    let osName: String?
    let arch: String?
    let pendingApprovals: Int
    let learnedSkills: Int?
    let proactiveSuggestions: Int?
    let mcpTools: Int?

    enum CodingKeys: String, CodingKey {
        case mascotName = "mascot_name"
        case deviceId = "device_id"
        case visualState = "visual_state"
        case decisionMode = "decision_mode"
        case layaEndpoint = "laya_endpoint"
        case autonomyLevel = "autonomy_level"
        case privacyShield = "privacy_shield"
        case killSwitch = "kill_switch"
        case llmProvider = "llm_provider"
        case llmConfigured = "llm_configured"
        case nexusmindConfigured = "nexusmind_configured"
        case nexusmindRequired = "nexusmind_required"
        case nexusmindConnected = "nexusmind_connected"
        case nexusmindError = "nexusmind_error"
        case hostname
        case osName = "os_name"
        case arch
        case pendingApprovals = "pending_approvals"
        case learnedSkills = "learned_skills"
        case proactiveSuggestions = "proactive_suggestions"
        case mcpTools = "mcp_tools"
    }
}

struct ConversationRecord: Codable, Identifiable {
    let id: String
    let createdAt: String
    let updatedAt: String
    let objective: String
    let status: String
    let route: String?
    let stepsExecuted: Int
    let toolErrors: Int
    let stateJson: String
    let finalReply: String?

    enum CodingKeys: String, CodingKey {
        case id, objective, status, route
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case stepsExecuted = "steps_executed"
        case toolErrors = "tool_errors"
        case stateJson = "state_json"
        case finalReply = "final_reply"
    }
}

struct ActivityLogEntry: Codable, Identifiable {
    let id: String
    let timestamp: String
    let runId: String?
    let eventKind: String
    let actionKind: String?
    let decisionSource: String?
    let risk: Double?
    let outcome: String?
    let summary: String
    let detailJson: String

    enum CodingKeys: String, CodingKey {
        case id, timestamp, risk, outcome, summary
        case runId = "run_id"
        case eventKind = "event_kind"
        case actionKind = "action_kind"
        case decisionSource = "decision_source"
        case detailJson = "detail_json"
    }
}

struct OsBridgeJob: Codable, Identifiable {
    let id: String
    let kind: String
    let payloadJson: String
    let status: String

    enum CodingKeys: String, CodingKey {
        case id, kind, status
        case payloadJson = "payload_json"
    }
}

struct ProactiveSuggestion: Codable, Identifiable {
    let id: String
    let title: String
    let summary: String
    let deliveryMode: String

    enum CodingKeys: String, CodingKey {
        case id, title, summary
        case deliveryMode = "delivery_mode"
    }
}

enum LlmProviderKind: String, Codable, CaseIterable, Identifiable {
    case claude
    case gemini
    case openAi = "open_ai"

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .claude: return "Claude (Anthropic)"
        case .gemini: return "Gemini (Google)"
        case .openAi: return "OpenAI (GPT)"
        }
    }

    var defaultModel: String {
        switch self {
        case .claude: return "claude-sonnet-4-5"
        case .gemini: return "gemini-2.5-pro"
        case .openAi: return "gpt-4.1"
        }
    }
}

struct MascotSettings: Codable {
    var mascotName: String
    var decisionMode: DecisionMode
    var layaEndpoint: String
    var llmProvider: LlmProviderKind
    var llmModel: String
    var nexusmindEnabled: Bool
    var nexusmindBaseUrl: String
    var nexusmindProject: String
    var hasAnthropicKey: Bool
    var hasGeminiKey: Bool
    var hasOpenAiKey: Bool
    var hasJevKey: Bool
    var hasNexusmindKey: Bool
    var nexusmindConnected: Bool?
    var nexusmindError: String?

    enum CodingKeys: String, CodingKey {
        case mascotName = "mascot_name"
        case decisionMode = "decision_mode"
        case layaEndpoint = "laya_endpoint"
        case llmProvider = "llm_provider"
        case llmModel = "llm_model"
        case nexusmindEnabled = "nexusmind_enabled"
        case nexusmindBaseUrl = "nexusmind_base_url"
        case nexusmindProject = "nexusmind_project"
        case hasAnthropicKey = "has_anthropic_key"
        case hasGeminiKey = "has_gemini_key"
        case hasOpenAiKey = "has_openai_key"
        case hasJevKey = "has_jev_key"
        case hasNexusmindKey = "has_nexusmind_key"
        case nexusmindConnected = "nexusmind_connected"
        case nexusmindError = "nexusmind_error"
    }
}

struct PendingApproval: Codable, Identifiable {
    let id: String
    let summary: String
    let actionKind: String
    let target: String?
    let commandPreview: String?
    let risk: Double
    let decisionSource: String
    let reason: String
    let status: String

    enum CodingKeys: String, CodingKey {
        case id, summary, target, risk, reason, status
        case actionKind = "action_kind"
        case commandPreview = "command_preview"
        case decisionSource = "decision_source"
    }
}

struct OutcomeEntry: Codable, Identifiable {
    let id: String
    let timestamp: String
    let summary: String
    let actionKind: String
    let decisionSource: String
    let risk: Double
    let outcome: String

    enum CodingKeys: String, CodingKey {
        case id, timestamp, summary, risk, outcome
        case actionKind = "action_kind"
        case decisionSource = "decision_source"
    }
}
