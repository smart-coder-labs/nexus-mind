import Foundation

#if canImport(Darwin)
import Darwin
#endif

/// Minimal local JSON-RPC 2.0 client speaking `mascot-ipc-v1` over `~/.nexusmind/mascot.sock`.
final class MascotDaemonClient: @unchecked Sendable {
    let socketPath: String

    init(socketPath: String? = nil) {
        if let path = socketPath {
            self.socketPath = path
        } else {
            let home = FileManager.default.homeDirectoryForCurrentUser.path
            self.socketPath = "\(home)/.nexusmind/mascot.sock"
        }
    }

    enum ClientError: Error, LocalizedError {
        case socketUnavailable(String)
        case invalidResponse
        case rpcError(String)

        var errorDescription: String? {
            switch self {
            case .socketUnavailable(let msg): return "Daemon no conectado: \(msg)"
            case .invalidResponse: return "Respuesta IPC inválida del daemon"
            case .rpcError(let msg): return msg
            }
        }
    }

    func getStatus() async throws -> DaemonStatus {
        let data = try await call(method: "get_status", params: [:])
        return try JSONDecoder().decode(DaemonStatus.self, from: data)
    }

    func setPrivacyShield(enabled: Bool) async throws {
        _ = try await call(method: "set_privacy_shield", params: ["enabled": enabled])
    }

    func setAutonomyLevel(_ level: AutonomyLevel) async throws {
        _ = try await call(method: "set_autonomy_level", params: ["level": level.rawValue])
    }

    func setDecisionMode(_ mode: DecisionMode) async throws {
        _ = try await call(method: "set_decision_mode", params: ["mode": mode.rawValue])
    }

    func setKillSwitch(active: Bool) async throws {
        _ = try await call(method: "kill_switch", params: ["active": active])
    }

    func listPendingApprovals() async throws -> [PendingApproval] {
        struct Wrapper: Codable { let approvals: [PendingApproval] }
        let data = try await call(method: "list_pending_approvals", params: [:])
        return try JSONDecoder().decode(Wrapper.self, from: data).approvals
    }

    func resolveApproval(id: String, approved: Bool) async throws {
        _ = try await call(
            method: "resolve_approval",
            params: ["id": id, "approved": approved]
        )
    }

    func getOutcomeTape(limit: Int = 14) async throws -> [OutcomeEntry] {
        struct Wrapper: Codable { let entries: [OutcomeEntry] }
        let data = try await call(method: "get_outcome_tape", params: ["limit": limit])
        return try JSONDecoder().decode(Wrapper.self, from: data).entries
    }

    func recordObservation(source: String, appName: String?, summary: String) async throws {
        var params: [String: Any] = ["source": source, "summary": summary]
        if let appName { params["app_name"] = appName }
        _ = try await call(method: "record_observation", params: params)
    }

    func submitPrompt(_ prompt: String, activeApp: String?) async throws -> String {
        struct ReplyWrapper: Codable {
            struct Reply: Codable { let text: String }
            let reply: Reply
        }
        var params: [String: Any] = ["prompt": prompt]
        if let activeApp { params["active_app"] = activeApp }
        let data = try await call(method: "submit_prompt", params: params)
        return try JSONDecoder().decode(ReplyWrapper.self, from: data).reply.text
    }

    func pollOsJobs() async throws -> [OsBridgeJob] {
        struct Wrapper: Codable { let jobs: [OsBridgeJob] }
        let data = try await call(method: "poll_os_jobs", params: [:])
        return try JSONDecoder().decode(Wrapper.self, from: data).jobs
    }

    func completeOsJob(id: String, ok: Bool, result: Any) async throws {
        _ = try await call(
            method: "complete_os_job",
            params: ["id": id, "ok": ok, "result": result]
        )
    }

    func listSuggestions() async throws -> [ProactiveSuggestion] {
        struct Wrapper: Codable { let suggestions: [ProactiveSuggestion] }
        let data = try await call(method: "list_suggestions", params: [:])
        return try JSONDecoder().decode(Wrapper.self, from: data).suggestions
    }

    private func call(method: String, params: [String: Any]) async throws -> Data {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                do {
                    let result = try self.sendSync(method: method, params: params)
                    continuation.resume(returning: result)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    private func sendSync(method: String, params: [String: Any]) throws -> Data {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else {
            throw ClientError.socketUnavailable("socket() failed")
        }
        defer { close(fd) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = socketPath.utf8CString
        let maxLen = MemoryLayout.size(ofValue: addr.sun_path)
        guard pathBytes.count <= maxLen else {
            throw ClientError.socketUnavailable("socket path too long")
        }
        withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
            ptr.withMemoryRebound(to: CChar.self, capacity: maxLen) { dst in
                for i in 0..<pathBytes.count {
                    dst[i] = pathBytes[i]
                }
            }
        }

        let connectResult = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                connect(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connectResult == 0 else {
            throw ClientError.socketUnavailable("No se pudo conectar a \(socketPath)")
        }

        let payload: [String: Any] = [
            "jsonrpc": "2.0",
            "id": UUID().uuidString,
            "method": method,
            "params": params
        ]
        var encoded = try JSONSerialization.data(withJSONObject: payload)
        encoded.append(0x0A) // newline delimiter

        _ = encoded.withUnsafeBytes { raw in
            write(fd, raw.baseAddress, encoded.count)
        }

        var responseBytes = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            let n = read(fd, &buffer, buffer.count)
            if n <= 0 { break }
            responseBytes.append(buffer, count: n)
            if buffer[0..<n].contains(0x0A) { break }
        }

        guard let obj = try JSONSerialization.jsonObject(with: responseBytes) as? [String: Any] else {
            throw ClientError.invalidResponse
        }
        if let err = obj["error"] as? [String: Any], let msg = err["message"] as? String {
            throw ClientError.rpcError(msg)
        }
        guard let resultObj = obj["result"] else {
            throw ClientError.invalidResponse
        }
        return try JSONSerialization.data(withJSONObject: resultObj)
    }
}
