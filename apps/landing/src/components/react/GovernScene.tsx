import { useEffect, useRef, useState } from 'react';
import { useScroll, useMotionValueEvent } from 'framer-motion';

/**
 * GovernScene — first cinematic scene: "Del caos al control".
 *
 * A pinned, scroll-scrubbed section that dramatizes the product story: scattered
 * ungoverned AI tools (chaos) → the robot gathers them → they snap into order
 * inside the NexusMind Control Plane. Robot uses real transparent frames
 * (walk → carry). Respects prefers-reduced-motion (renders the resolved end
 * state, no pinning drama).
 */

const BASE = '/robot/frames';
const ROBOT_H = 210;

const TOOLS = [
  { name: 'Claude Code', chaos: { x: 16, y: 24, r: -13 } },
  { name: 'Cursor', chaos: { x: 44, y: 60, r: 10 } },
  { name: 'Copilot', chaos: { x: 12, y: 66, r: 15 } },
  { name: 'OpenCode', chaos: { x: 50, y: 20, r: -9 } },
  { name: 'Cline', chaos: { x: 33, y: 42, r: 7 } },
  { name: 'CrewAI', chaos: { x: 26, y: 50, r: -17 } },
];

const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
const smooth = (t: number) => t * t * (3 - 2 * t);

function robotState(p: number): { state: 'walk' | 'carry'; x: number } {
  // x = horizontal % of the stage where the robot stands.
  if (p < 0.46) return { state: 'walk', x: lerp(7, 32, p / 0.46) };
  if (p < 0.6) return { state: 'carry', x: lerp(32, 42, (p - 0.46) / 0.14) };
  return { state: 'carry', x: lerp(42, 66, clamp01((p - 0.6) / 0.4)) };
}

export default function GovernScene() {
  const sectionRef = useRef<HTMLDivElement>(null);
  const { scrollYProgress } = useScroll({
    target: sectionRef,
    offset: ['start start', 'end end'],
  });

  const [p, setP] = useState(0);
  const [frames, setFrames] = useState<Record<string, string[]>>({});
  const [frame, setFrame] = useState(0);
  const reduced = useRef(false);

  useEffect(() => {
    // `?forceP=<0..1>` freezes the scene at a given progress — used to capture
    // specific states into Figma (a scroll-scrubbed scene can't be captured live).
    const forced = new URLSearchParams(window.location.search).get('forceP');
    const rm = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (forced != null) {
      reduced.current = true;
      setP(clamp01(parseFloat(forced)));
    } else if (rm) {
      reduced.current = true;
      setP(0.9);
    }
    fetch(`${BASE}/manifest.json`)
      .then((r) => (r.ok ? r.json() : null))
      .then((m) => {
        if (!m) return;
        const built: Record<string, string[]> = {};
        (['walk', 'carry'] as const).forEach((s) => {
          const n = m.states[s]?.frames ?? 0;
          built[s] = Array.from(
            { length: n },
            (_, k) => `${BASE}/${s}/${s}-${String(k + 1).padStart(2, '0')}.png`,
          );
          built[s].forEach((src) => {
            const im = new Image();
            im.src = src;
          });
        });
        setFrames(built);
      })
      .catch(() => {});
  }, []);

  useMotionValueEvent(scrollYProgress, 'change', (v) => {
    if (!reduced.current) setP(v);
  });

  useEffect(() => {
    if (reduced.current) return;
    let raf = 0;
    let last = 0;
    const tick = (t: number) => {
      if (t - last > 90) {
        last = t;
        setFrame((f) => f + 1);
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  const { state, x } = robotState(p);
  const stateFrames = frames[state] ?? [];
  const robotSrc = stateFrames.length ? stateFrames[frame % stateFrames.length] : null;

  const orderT = smooth(clamp01((p - 0.45) / 0.35)); // chaos → order
  const aOpacity = 1 - clamp01((p - 0.4) / 0.15); // problema headline
  const bOpacity = clamp01((p - 0.52) / 0.16); // solución headline
  const panelOn = clamp01((orderT - 0.15) / 0.6);

  return (
    <section
      ref={sectionRef}
      id="scene-govern"
      aria-label="Del caos al control: NexusMind gobierna tus herramientas AI"
      style={{
        position: 'relative',
        height: reduced.current ? 'auto' : '280vh',
        background: 'var(--color-bg-primary)',
      }}
    >
      <div
        style={{
          position: reduced.current ? 'relative' : 'sticky',
          top: 0,
          minHeight: reduced.current ? undefined : '100vh',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          overflow: 'hidden',
          padding: reduced.current ? '6rem 1rem' : '0 1rem',
        }}
      >
        {/* Ambient glow that intensifies as order emerges */}
        <div
          aria-hidden="true"
          style={{
            position: 'absolute',
            inset: 0,
            background: `radial-gradient(60% 50% at 72% 45%, rgba(0,102,204,${0.05 + orderT * 0.22}), transparent 70%)`,
            transition: 'background 0.1s linear',
          }}
        />

        {/* Headlines (crossfade A → B) */}
        <div style={{ position: 'relative', width: 'min(920px, 92%)', height: 140, marginBottom: '1.75rem', zIndex: 3 }}>
          <h2
            className="font-display text-3xl sm:text-4xl md:text-5xl font-bold"
            style={{ position: 'absolute', left: 0, right: 0, top: 0, margin: 0, textAlign: 'center', color: 'var(--color-text-primary)', opacity: aOpacity, transition: 'opacity 0.1s linear' }}
          >
            Tus herramientas AI:<br />dispersas, sin memoria, sin gobierno.
          </h2>
          <h2
            className="font-display text-3xl sm:text-4xl md:text-5xl font-bold"
            style={{ position: 'absolute', left: 0, right: 0, top: 0, margin: 0, textAlign: 'center', opacity: bOpacity, transition: 'opacity 0.1s linear' }}
          >
            <span style={{ color: 'var(--color-text-primary)' }}>Un </span>
            <span className="text-gradient-accent">control plane</span>
            <span style={{ color: 'var(--color-text-primary)' }}> las gobierna a todas.</span>
          </h2>
        </div>

        {/* Stage */}
        <div style={{ position: 'relative', width: 'min(1080px, 96%)', height: '58vh', minHeight: 380 }}>
          {/* Control Plane panel (target of order) */}
          <div
            style={{
              position: 'absolute',
              right: '2%',
              top: '8%',
              width: '34%',
              height: '84%',
              borderRadius: 20,
              border: `1px solid rgba(0,102,204,${0.2 + panelOn * 0.4})`,
              background: `linear-gradient(180deg, rgba(0,102,204,${0.04 + panelOn * 0.08}), rgba(10,15,26,0.6))`,
              boxShadow: `0 30px 80px -30px rgba(0,102,204,${panelOn * 0.5})`,
              opacity: 0.25 + panelOn * 0.75,
              backdropFilter: 'blur(2px)',
            }}
          >
            <div
              className="font-display"
              style={{ textAlign: 'center', paddingTop: 14, fontWeight: 700, fontSize: 13, color: 'var(--color-accent-blue)', opacity: panelOn }}
            >
              NexusMind Control Plane
            </div>
          </div>

          {/* Tool chips: chaos → ordered list inside the panel */}
          {TOOLS.map((tool, i) => {
            const orderedX = 69; // % (inside panel)
            const orderedY = 22 + i * 11;
            const cx = lerp(tool.chaos.x, orderedX, orderT);
            const cy = lerp(tool.chaos.y, orderedY, orderT);
            const rot = lerp(tool.chaos.r, 0, orderT);
            const opacity = 0.45 + orderT * 0.55;
            return (
              <div
                key={tool.name}
                style={{
                  position: 'absolute',
                  left: `${cx}%`,
                  top: `${cy}%`,
                  transform: `translate(-50%, -50%) rotate(${rot}deg)`,
                  fontSize: 13,
                  padding: '6px 14px',
                  borderRadius: 999,
                  whiteSpace: 'nowrap',
                  color: orderT > 0.6 ? 'var(--color-text-primary)' : 'var(--color-text-secondary)',
                  background: orderT > 0.6 ? 'rgba(0,102,204,0.12)' : 'rgba(255,255,255,0.05)',
                  border: `1px solid ${orderT > 0.6 ? 'rgba(0,102,204,0.35)' : 'rgba(255,255,255,0.1)'}`,
                  opacity,
                  transition: 'color 0.2s, background 0.2s, border-color 0.2s',
                  zIndex: 2,
                }}
              >
                {tool.name}
              </div>
            );
          })}

          {/* Robot */}
          {robotSrc && (
            <div
              style={{
                position: 'absolute',
                bottom: '2%',
                left: `${x}%`,
                transform: `translateX(-50%) scaleX(-1)`,
                transition: 'left 0.12s linear',
                zIndex: 4,
              }}
            >
              <img
                src={robotSrc}
                alt=""
                aria-hidden="true"
                style={{ height: ROBOT_H, width: 'auto', filter: 'drop-shadow(0 16px 26px rgba(0,0,0,0.5))' }}
              />
            </div>
          )}
        </div>

        <p className="mt-6 text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
          {orderT > 0.9 ? 'Memoria, políticas y trazabilidad — para todas.' : 'Desplázate ↓'}
        </p>
      </div>
    </section>
  );
}
