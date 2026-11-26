import Carbon
import Foundation

/// Global macOS hotkeys using Carbon `RegisterEventHotKey` (works system-wide across all apps):
/// - `⌘⇧N` (kVK_ANSI_N): Toggle Quick Command Bar
/// - `⌘⇧V` (kVK_ANSI_V): Toggle Voice / Microphone Input
/// - `⌘⇧P` (kVK_ANSI_P): Toggle Privacy Shield
/// - `⌘⇧⎋` (kVK_Escape): Emergency Kill Switch
final class GlobalHotkeys {
    static let shared = GlobalHotkeys()

    var onQuickCommand: (() -> Void)?
    var onVoiceToggle: (() -> Void)?
    var onPrivacyShield: (() -> Void)?
    var onKillSwitch: (() -> Void)?

    private var refs: [EventHotKeyRef] = []
    private var handlerRef: EventHandlerRef?

    func registerAll() {
        guard handlerRef == nil else { return }

        var eventType = EventTypeSpec(
            eventClass: OSType(kEventClassKeyboard),
            eventKind: UInt32(kEventHotKeyPressed)
        )

        InstallEventHandler(
            GetApplicationEventTarget(),
            { _, event, _ in
                var hkID = EventHotKeyID()
                let err = GetEventParameter(
                    event,
                    EventParamName(kEventParamDirectObject),
                    EventParamType(typeEventHotKeyID),
                    nil,
                    MemoryLayout<EventHotKeyID>.size,
                    nil,
                    &hkID
                )
                guard err == noErr else { return noErr }
                DispatchQueue.main.async {
                    switch hkID.id {
                    case 1: GlobalHotkeys.shared.onQuickCommand?()
                    case 2: GlobalHotkeys.shared.onPrivacyShield?()
                    case 3: GlobalHotkeys.shared.onKillSwitch?()
                    case 4: GlobalHotkeys.shared.onVoiceToggle?()
                    default: break
                    }
                }
                return noErr
            },
            1,
            &eventType,
            nil,
            &handlerRef
        )

        let modifiers = UInt32(cmdKey | shiftKey)
        register(id: 1, keyCode: UInt32(kVK_ANSI_N), modifiers: modifiers)
        register(id: 2, keyCode: UInt32(kVK_ANSI_P), modifiers: modifiers)
        register(id: 3, keyCode: UInt32(kVK_Escape), modifiers: modifiers)
        register(id: 4, keyCode: UInt32(kVK_ANSI_V), modifiers: modifiers)
    }

    private func register(id: UInt32, keyCode: UInt32, modifiers: UInt32) {
        var ref: EventHotKeyRef?
        let hkID = EventHotKeyID(signature: OSType(0x4E4D5343), id: id) // 'NMSC'
        let status = RegisterEventHotKey(keyCode, modifiers, hkID, GetApplicationEventTarget(), 0, &ref)
        if status == noErr, let ref {
            refs.append(ref)
        }
    }

    deinit {
        for ref in refs {
            UnregisterEventHotKey(ref)
        }
        if let handlerRef {
            RemoveEventHandler(handlerRef)
        }
    }
}
