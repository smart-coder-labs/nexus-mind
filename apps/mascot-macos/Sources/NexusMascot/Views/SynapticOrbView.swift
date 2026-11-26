import SwiftUI

/// Animated minimalist "Synaptic Orb" mascot built natively in SwiftUI.
/// Uses invariant layout geometry (zero bounding-box jitter) and smooth GPU rotation.
struct SynapticOrbView: View {
    let state: MascotVisualState
    var size: CGFloat = 76
    var animated: Bool = true

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var spin: Bool = false

    private var primaryGlow: Color {
        switch state {
        case .idle, .listening, .acting:
            return NexusTokens.accentLime
        case .deciding, .thinking:
            return NexusTokens.statusBlue
        case .learning:
            return NexusTokens.statusTeal
        case .held:
            return NexusTokens.statusAmber
        case .privacyShield:
            return NexusTokens.statusSlate
        case .blocked:
            return NexusTokens.statusRose
        }
    }

    var body: some View {
        ZStack {
            // Soft ambient halo contained inside fixed bounds
            if state != .privacyShield {
                Circle()
                    .fill(
                        RadialGradient(
                            colors: [primaryGlow.opacity(0.32), primaryGlow.opacity(0.0)],
                            center: .center,
                            startRadius: size * 0.15,
                            endRadius: size * 0.48
                        )
                    )
                    .frame(width: size * 0.96, height: size * 0.96)
            }

            // Orbital ring & synaptic nodes (fixed frame, pure GPU rotation)
            if state != .privacyShield {
                orbitRing
            }

            // Obsidian orb body
            Circle()
                .fill(
                    RadialGradient(
                        colors: [
                            Color(white: 0.24),
                            NexusTokens.bodyObsidian,
                            Color(white: 0.06)
                        ],
                        center: .init(x: 0.35, y: 0.28),
                        startRadius: size * 0.04,
                        endRadius: size * 0.50
                    )
                )
                .frame(width: size * 0.66, height: size * 0.66)
                .overlay(
                    Circle()
                        .strokeBorder(
                            LinearGradient(
                                colors: [Color.white.opacity(0.22), Color.white.opacity(0.04)],
                                startPoint: .topLeading,
                                endPoint: .bottomTrailing
                            ),
                            lineWidth: 1
                        )
                )

            // Recessed dark visor
            Circle()
                .fill(Color(white: 0.07))
                .frame(width: size * 0.46, height: size * 0.46)
                .overlay(
                    Circle()
                        .stroke(Color.white.opacity(0.08), lineWidth: 0.75)
                )

            // Expressive eyes
            eyesView
        }
        .frame(width: size, height: size)
        .contentShape(Circle())
        .clipped()
        .onAppear {
            guard animated, !reduceMotion else { return }
            withAnimation(.linear(duration: orbitDuration).repeatForever(autoreverses: false)) {
                spin = true
            }
        }
        .accessibilityLabel("NexusMind Mascot: \(state.label)")
    }

    private var orbitDuration: Double {
        switch state {
        case .deciding: return 2.0
        case .thinking, .learning, .acting: return 4.0
        default: return 9.0
        }
    }

    private var orbitRing: some View {
        ZStack {
            Circle()
                .stroke(primaryGlow.opacity(state == .idle ? 0.26 : 0.65), lineWidth: 1.2)
                .frame(width: size * 0.84, height: size * 0.84)

            // Three evenly spaced synaptic nodes rotated cleanly on GPU
            ZStack {
                ForEach(0..<3, id: \.self) { idx in
                    Circle()
                        .fill(idx == 1 && state == .thinking ? NexusTokens.statusTeal : primaryGlow)
                        .frame(width: size * 0.085, height: size * 0.085)
                        .offset(y: -size * 0.42)
                        .rotationEffect(.degrees(Double(idx) * 120.0))
                }
            }
            .rotationEffect(.degrees(spin ? 360 : 0))
        }
        .scaleEffect(x: 1.0, y: 0.46)
        .rotationEffect(.degrees(-16))
        .frame(width: size, height: size)
    }

    @ViewBuilder
    private var eyesView: some View {
        if state == .privacyShield {
            HStack(spacing: size * 0.10) {
                ClosedEyeArc()
                    .stroke(NexusTokens.statusSlate, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .frame(width: size * 0.11, height: size * 0.055)
                ClosedEyeArc()
                    .stroke(NexusTokens.statusSlate, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .frame(width: size * 0.11, height: size * 0.055)
            }
        } else {
            HStack(spacing: size * 0.10) {
                Capsule()
                    .fill(primaryGlow)
                    .frame(width: size * 0.10, height: eyeHeight)
                Capsule()
                    .fill(primaryGlow)
                    .frame(width: size * 0.10, height: eyeHeight)
            }
            .offset(y: state == .thinking ? -size * 0.025 : 0)
        }
    }

    private var eyeHeight: CGFloat {
        switch state {
        case .listening, .held: return size * 0.135
        case .blocked: return size * 0.055
        default: return size * 0.115
        }
    }
}

private struct ClosedEyeArc: Shape {
    func path(in rect: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: rect.minX, y: rect.minY))
        p.addQuadCurve(
            to: CGPoint(x: rect.maxX, y: rect.minY),
            control: CGPoint(x: rect.midX, y: rect.maxY)
        )
        return p
    }
}
