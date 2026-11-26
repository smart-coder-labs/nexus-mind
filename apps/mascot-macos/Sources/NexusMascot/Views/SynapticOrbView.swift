import SwiftUI

/// Animated minimalist "Synaptic Orb" mascot built natively in SwiftUI.
/// Visually mirrors the NexusMind concept sheet: obsidian metallic sphere,
/// expressive visor eyes, and orbital synapse nodes driven by live daemon state.
struct SynapticOrbView: View {
    let state: MascotVisualState
    var size: CGFloat = 76

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var orbitPhase: Double = 0
    @State private var breathe: Bool = false

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
            // Subtle bottom/ambient glow
            if state != .privacyShield {
                Circle()
                    .fill(primaryGlow.opacity(breathe ? 0.28 : 0.14))
                    .frame(width: size * 0.92, height: size * 0.92)
                    .blur(radius: size * 0.16)
            }

            // Orbital ring & synaptic nodes
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
                        endRadius: size * 0.56
                    )
                )
                .frame(width: size * 0.72, height: size * 0.72)
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
                .frame(width: size * 0.50, height: size * 0.50)
                .overlay(
                    Circle()
                        .stroke(Color.white.opacity(0.08), lineWidth: 0.75)
                )

            // Expressive eyes
            eyesView
        }
        .frame(width: size, height: size)
        .scaleEffect(!reduceMotion && breathe ? 1.03 : 1.0)
        .onAppear {
            guard !reduceMotion else { return }
            withAnimation(.easeInOut(duration: 2.2).repeatForever(autoreverses: true)) {
                breathe = true
            }
            withAnimation(.linear(duration: orbitDuration).repeatForever(autoreverses: false)) {
                orbitPhase = 360
            }
        }
        .accessibilityLabel("NexusMind Mascot: \(state.label)")
    }

    private var orbitDuration: Double {
        switch state {
        case .deciding: return 1.8
        case .thinking, .learning, .acting: return 3.6
        default: return 8.0
        }
    }

    private var orbitRing: some View {
        ZStack {
            Ellipse()
                .stroke(primaryGlow.opacity(state == .idle ? 0.22 : 0.65), lineWidth: 1.2)
                .frame(width: size * 0.96, height: size * 0.42)
                .rotationEffect(.degrees(-16))

            ForEach(0..<3, id: \.self) { idx in
                let angle = (orbitPhase + Double(idx) * 120.0) * .pi / 180.0
                let rx = size * 0.46
                let ry = size * 0.20
                let x = cos(angle) * rx
                let y = sin(angle) * ry
                Circle()
                    .fill(idx == 1 && state == .thinking ? NexusTokens.statusTeal : primaryGlow)
                    .frame(width: size * 0.085, height: size * 0.085)
                    .shadow(color: primaryGlow.opacity(0.75), radius: 3)
                    .offset(x: x, y: y)
                    .rotationEffect(.degrees(-16))
            }
        }
    }

    @ViewBuilder
    private var eyesView: some View {
        if state == .privacyShield {
            // Peaceful closed eyelids
            HStack(spacing: size * 0.11) {
                ClosedEyeArc()
                    .stroke(NexusTokens.statusSlate, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .frame(width: size * 0.12, height: size * 0.06)
                ClosedEyeArc()
                    .stroke(NexusTokens.statusSlate, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .frame(width: size * 0.12, height: size * 0.06)
            }
        } else {
            HStack(spacing: size * 0.11) {
                Capsule()
                    .fill(primaryGlow)
                    .frame(width: size * 0.11, height: eyeHeight)
                    .shadow(color: primaryGlow.opacity(0.65), radius: 4)
                Capsule()
                    .fill(primaryGlow)
                    .frame(width: size * 0.11, height: eyeHeight)
                    .shadow(color: primaryGlow.opacity(0.65), radius: 4)
            }
            .offset(y: state == .thinking ? -size * 0.03 : 0)
        }
    }

    private var eyeHeight: CGFloat {
        switch state {
        case .listening, .held: return size * 0.145
        case .blocked: return size * 0.06
        default: return size * 0.125
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
