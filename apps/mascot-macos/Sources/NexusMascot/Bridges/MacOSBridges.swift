import AppKit
import ApplicationServices
import Foundation
import ScreenCaptureKit

struct PermissionsSnapshot {
    let accessibilityGranted: Bool
    let screenRecordingGranted: Bool

    var allCoreGranted: Bool {
        accessibilityGranted && screenRecordingGranted
    }
}

enum MacOSPermissions {
    static func snapshot() -> PermissionsSnapshot {
        let ax = AXIsProcessTrusted()
        let screen = CGPreflightScreenCaptureAccess()
        return PermissionsSnapshot(accessibilityGranted: ax, screenRecordingGranted: screen)
    }

    static func requestAccessibility() {
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
        _ = AXIsProcessTrustedWithOptions(options)
    }

    static func openScreenRecordingSettings() {
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture") {
            NSWorkspace.shared.open(url)
        }
    }
}

/// Observes frontmost application changes via `NSWorkspace` (opt-in passive context sensor).
final class ActiveContextObserver {
    private var token: NSObjectProtocol?

    func start(onAppChange: @escaping @Sendable (String) -> Void) {
        token = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification,
            object: nil,
            queue: .main
        ) { notification in
            guard
                let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
                let name = app.localizedName
            else { return }
            onAppChange(name)
        }
    }

    func stop() {
        if let token {
            NSWorkspace.shared.notificationCenter.removeObserver(token)
        }
        token = nil
    }
}

/// Tier 3 macOS Accessibility Bridge (`AXUIElement`) for semantic UI inspection & control.
enum AccessibilityBridge {
    static func frontmostWindowTitle() -> String? {
        guard AXIsProcessTrusted(),
              let app = NSWorkspace.shared.frontmostApplication
        else { return nil }

        let appElement = AXUIElementCreateApplication(app.processIdentifier)
        var focusedWindow: CFTypeRef?
        guard AXUIElementCopyAttributeValue(
            appElement,
            kAXFocusedWindowAttribute as CFString,
            &focusedWindow
        ) == .success, let window = focusedWindow else {
            return nil
        }

        var titleValue: CFTypeRef?
        guard AXUIElementCopyAttributeValue(
            window as! AXUIElement,
            kAXTitleAttribute as CFString,
            &titleValue
        ) == .success else {
            return nil
        }
        return titleValue as? String
    }

    /// Captures a bounded semantic snapshot of interactive UI nodes in the frontmost window.
    static func captureSemanticTree(maxNodes: Int = 80) -> [[String: String]] {
        guard AXIsProcessTrusted(),
              let app = NSWorkspace.shared.frontmostApplication
        else { return [] }

        let appElement = AXUIElementCreateApplication(app.processIdentifier)
        var focusedWindow: CFTypeRef?
        guard AXUIElementCopyAttributeValue(
            appElement,
            kAXFocusedWindowAttribute as CFString,
            &focusedWindow
        ) == .success, let window = focusedWindow else {
            return []
        }

        var results: [[String: String]] = []
        var queue: [AXUIElement] = [window as! AXUIElement]

        while !queue.isEmpty && results.count < maxNodes {
            let current = queue.removeFirst()
            let role = stringAttr(current, kAXRoleAttribute) ?? ""
            let title = stringAttr(current, kAXTitleAttribute) ?? ""
            let desc = stringAttr(current, kAXDescriptionAttribute) ?? ""

            // Never inspect secure password text fields!
            if role == "AXSecureTextField" {
                continue
            }

            if !title.isEmpty || !desc.isEmpty {
                results.append([
                    "role": role,
                    "title": title,
                    "description": desc
                ])
            }

            var childrenRef: CFTypeRef?
            if AXUIElementCopyAttributeValue(current, kAXChildrenAttribute as CFString, &childrenRef) == .success,
               let children = childrenRef as? [AXUIElement] {
                queue.append(contentsOf: children.prefix(25))
            }
        }
        return results
    }

    /// Performs a semantic `AXPress` on the first element matching `label` in the frontmost window.
    static func pressElement(matching label: String) -> Bool {
        guard AXIsProcessTrusted(),
              let app = NSWorkspace.shared.frontmostApplication
        else { return false }

        let appElement = AXUIElementCreateApplication(app.processIdentifier)
        var focusedWindow: CFTypeRef?
        guard AXUIElementCopyAttributeValue(
            appElement,
            kAXFocusedWindowAttribute as CFString,
            &focusedWindow
        ) == .success, let window = focusedWindow else {
            return false
        }

        let needle = label.lowercased()
        var queue: [AXUIElement] = [window as! AXUIElement]
        var visited = 0
        while !queue.isEmpty && visited < 200 {
            visited += 1
            let el = queue.removeFirst()
            let role = stringAttr(el, kAXRoleAttribute) ?? ""
            if role == "AXSecureTextField" { continue }

            let title = (stringAttr(el, kAXTitleAttribute) ?? "").lowercased()
            let desc = (stringAttr(el, kAXDescriptionAttribute) ?? "").lowercased()
            if title == needle || desc == needle {
                return AXUIElementPerformAction(el, kAXPressAction as CFString) == .success
            }

            var childrenRef: CFTypeRef?
            if AXUIElementCopyAttributeValue(el, kAXChildrenAttribute as CFString, &childrenRef) == .success,
               let children = childrenRef as? [AXUIElement] {
                queue.append(contentsOf: children)
            }
        }
        return false
    }

    private static func stringAttr(_ el: AXUIElement, _ attr: String) -> String? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, attr as CFString, &value) == .success else {
            return nil
        }
        return value as? String
    }
}

/// Tier 4 Visual ScreenCaptureKit single-frame screenshot for VLM grounding.
enum ScreenCaptureBridge {
    static func captureMainDisplayBase64PNG() async throws -> String {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first else {
            throw NSError(domain: "ScreenCaptureBridge", code: 1, userInfo: [NSLocalizedDescriptionKey: "No active display"])
        }
        let filter = SCContentFilter(display: display, excludingWindows: [])
        let config = SCStreamConfiguration()
        config.width = min(display.width, 1440)
        config.height = min(display.height, 900)
        let cgImage = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
        let rep = NSBitmapImageRep(cgImage: cgImage)
        guard let png = rep.representation(using: .png, properties: [:]) else {
            throw NSError(domain: "ScreenCaptureBridge", code: 2, userInfo: [NSLocalizedDescriptionKey: "PNG encoding failed"])
        }
        return png.base64EncodedString()
    }
}

/// Tier 4 Low-level Event Actuator (`CGEvent`) — only invoked after daemon Policy Engine approves.
enum EventActuator {
    static func leftClick(at point: CGPoint) {
        guard AXIsProcessTrusted() else { return }
        let down = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: point, mouseButton: .left)
        let up = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: point, mouseButton: .left)
        down?.post(tap: .cghidEventTap)
        up?.post(tap: .cghidEventTap)
    }

    static func typeText(_ text: String) {
        guard AXIsProcessTrusted() else { return }
        let utf16 = Array(text.utf16)
        guard let keyDown = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true),
              let keyUp = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false)
        else { return }
        keyDown.keyboardSetUnicodeString(stringLength: utf16.count, unicodeString: utf16)
        keyUp.keyboardSetUnicodeString(stringLength: utf16.count, unicodeString: utf16)
        keyDown.post(tap: .cghidEventTap)
        keyUp.post(tap: .cghidEventTap)
    }
}
