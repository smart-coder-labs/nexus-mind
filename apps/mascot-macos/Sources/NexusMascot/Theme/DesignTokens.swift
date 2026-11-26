import SwiftUI

/// NexusMind Audit1 Lime design tokens (aligned with `apps/admin/DESIGN.md` & `docs/DESIGN_DIRECTION.md`).
enum NexusTokens {
    // Surfaces
    static let bodyObsidian = Color(red: 0.110, green: 0.110, blue: 0.110) // #1c1c1c
    static let surfaceElevated = Color(red: 0.145, green: 0.157, blue: 0.161) // #252829
    static let surfaceBorder = Color.white.opacity(0.11)

    // Primary Accent (Audit1 Lime)
    static let accentLime = Color(red: 0.765, green: 0.847, blue: 0.180) // #c3d82e
    static let accentLimeHover = Color(red: 0.804, green: 0.894, blue: 0.353) // #cde45a
    static let accentLimeForeground = Color(red: 0.114, green: 0.141, blue: 0.035) // #1d2409

    // Semantic Status Colors (state only, never decorative)
    static let statusSuccess = Color(red: 0.518, green: 0.902, blue: 0.690) // #84e6b0
    static let statusTeal = Color(red: 0.510, green: 0.796, blue: 0.749) // #82cbbf
    static let statusBlue = Color(red: 0.608, green: 0.784, blue: 0.929) // #9bc8ed
    static let statusAmber = Color(red: 0.902, green: 0.757, blue: 0.427) // #e6c16d
    static let statusRose = Color(red: 0.878, green: 0.627, blue: 0.678) // #e0a0ad
    static let statusSlate = Color(red: 0.520, green: 0.545, blue: 0.575)

    // Radius grammar
    static let radiusControl: CGFloat = 8
    static let radiusInput: CGFloat = 11
    static let radiusCard: CGFloat = 18
}
