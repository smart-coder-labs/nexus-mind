import { useEffect, useRef, useState } from 'react';
import { useScroll, useMotionValueEvent } from 'framer-motion';

/**
 * RobotMigration — scroll-driven mascot scene.
 *
 * The robot walks a "migration stage" that reuses NexusMind's own narrative:
 * scattered, ungoverned tools on the left → the governed Control Plane on the
 * right. As the user scrolls the (tall) section, a sticky stage stays pinned
 * and the robot cycles through states: walk → scan → carry → transfer → done →
 * celebrate, while a progress readout fills.
 *
 * ASSET NOTE: frames are cropped at runtime from /robot/sprite-bot.png (a single
 * 1536×1024 sheet) via CSS background-position — no per-frame files or alpha are
 * needed because the stage panel replicates the sprite sheet's blue ambient, so
 * the (non-transparent) frame backgrounds blend into it. Coordinates below were
 * verified visually against the sheet.
 */

const SHEET = { w: 1536, h: 1024, src: '/robot/sprite-bot.png' };

// [sx, sy, sw, sh] rectangles into the sheet. Verified via visual probe.
type Rect = [number, number, number, number];
const FRAMES: Record<string, Rect[]> = {
  // Centered on individual robots (sw=74) to avoid bleeding the neighbour frame.
  walk: [
    [113, 6, 74, 162],
    [207, 6, 74, 162],
    [301, 6, 74, 162],
    [583, 6, 74, 162],
  ],
  scan: [
    [460, 196, 150, 165],
    [610, 196, 150, 165],
  ],
  carry: [
    [800, 380, 144, 165],
    [944, 380, 144, 165],
    [1088, 380, 144, 165],
    [1232, 380, 144, 165],
    [1376, 380, 144, 165],
  ],
  // Single clean "placing the box into the server" unit (no neighbour bleed).
  transfer: [[470, 556, 180, 150]],
  done: [[1300, 556, 200, 150]],
  celebrate: [
    [980, 694, 180, 156],
    [1160, 694, 180, 156],
  ],
  trophy: [[1340, 694, 180, 156]],
};

type Phase = {
  state: keyof typeof FRAMES;
  status: string;
  // horizontal position of the robot on the stage, in % of stage width
  x: number;
  // whether the robot is facing right (default) — celebrate/scan face forward
  flip?: boolean;
};

/**
 * Map scroll progress p ∈ [0,1] to a discrete scene phase.
 * Movement happens during walk/carry; the robot pauses to scan/transfer/celebrate.
 */
function phaseFor(p: number): Phase {
  if (p < 0.12) {
    // walk in from the left toward the "ungoverned" cluster
    const t = p / 0.12;
    return { state: 'walk', status: 'Explorando herramientas dispersas', x: 8 + t * 18 };
  }
  if (p < 0.28) return { state: 'scan', status: 'Escaneando conocimiento sin gobierno', x: 26 };
  if (p < 0.5) {
    // carry the data box toward the control plane
    const t = (p - 0.28) / 0.22;
    return { state: 'carry', status: 'Consolidando memoria', x: 26 + t * 40 };
  }
  if (p < 0.72) return { state: 'transfer', status: 'Transfiriendo al Control Plane', x: 66 };
  if (p < 0.8) return { state: 'done', status: 'Aplicando políticas de gobierno', x: 66 };
  if (p < 0.92) return { state: 'celebrate', status: '¡Migración completa!', x: 68, flip: false };
  return { state: 'trophy', status: '¡Migración completa!', x: 68, flip: false };
}

/** Migrated % readout: ramps while carrying/transferring, then holds at 100. */
function progressFor(p: number): number {
  if (p < 0.28) return 0;
  if (p >= 0.72) return 100;
  return Math.round(((p - 0.28) / (0.72 - 0.28)) * 100);
}

const DISPLAY_H = 300; // on-screen robot height in px (desktop)

export default function RobotMigration() {
  const sectionRef = useRef<HTMLDivElement>(null);
  const { scrollYProgress } = useScroll({
    target: sectionRef,
    offset: ['start start', 'end end'],
  });

  const [p, setP] = useState(0);
  const [frame, setFrame] = useState(0);
  const [reducedMotion, setReducedMotion] = useState(false);
  const reduced = useRef(false);

  useEffect(() => {
    reduced.current = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (reduced.current) {
      setReducedMotion(true);
      setP(0.95); // static end-state: complete
    }
  }, []);

  useMotionValueEvent(scrollYProgress, 'change', (v) => {
    if (!reduced.current) setP(v);
  });

  // Frame cycling within the current action (independent of scroll), ~8fps.
  useEffect(() => {
    if (reduced.current) return;
    let raf = 0;
    let last = 0;
    const tick = (t: number) => {
      if (t - last > 120) {
        last = t;
        setFrame((f) => f + 1);
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  const phase = phaseFor(p);
  const progress = progressFor(p);
  const complete = progress >= 100;
  const rects = FRAMES[phase.state];
  const rect = rects[frame % rects.length];
  const [sx, sy, sw, sh] = rect;

  // Derive the CSS crop for the active frame, normalized to DISPLAY_H.
  const scale = DISPLAY_H / sh;
  const spriteStyle: React.CSSProperties = {
    width: sw * scale,
    height: sh * scale,
    backgroundImage: `url(${SHEET.src})`,
    backgroundRepeat: 'no-repeat',
    backgroundSize: `${SHEET.w * scale}px ${SHEET.h * scale}px`,
    backgroundPosition: `${-sx * scale}px ${-sy * scale}px`,
    // Fade ONLY the outermost rectangle border so the sheet ambient reads as a
    // glow — kept large/soft so it never clips the robot or the carried box.
    WebkitMaskImage:
      'radial-gradient(ellipse 98% 94% at 50% 50%, #000 86%, transparent 100%)',
    maskImage: 'radial-gradient(ellipse 98% 94% at 50% 50%, #000 86%, transparent 100%)',
    transform: phase.flip ? 'scaleX(-1)' : undefined,
    transition: 'transform 0.2s ease',
  };

  return (
    <section
      ref={sectionRef}
      aria-label="Animación: NexusMind migrando conocimiento al control plane"
      style={{
        position: 'relative',
        // Reduced motion → static card (no tall scroll-scrub region).
        height: reducedMotion ? 'auto' : '300vh',
        background: 'var(--color-bg-primary)',
      }}
    >
      <div
        style={{
          position: reducedMotion ? 'relative' : 'sticky',
          top: 0,
          minHeight: reducedMotion ? undefined : '100vh',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          overflow: 'hidden',
          padding: reducedMotion ? '5rem 1rem' : '0 1rem',
        }}
      >
        {/* Heading */}
        <div style={{ textAlign: 'center', marginBottom: '1.5rem', zIndex: 2 }}>
          <span className="tile-label" style={{ color: 'var(--color-accent-blue)' }}>
            En acción
          </span>
          <h2
            className="text-3xl sm:text-4xl font-bold mt-3"
            style={{ color: 'var(--color-text-primary)' }}
          >
            De herramientas dispersas a un cerebro gobernado
          </h2>
        </div>

        {/* Stage — its blue ambient replicates the sprite sheet background */}
        <div
          style={{
            position: 'relative',
            width: 'min(960px, 100%)',
            height: 360,
            borderRadius: 24,
            border: '1px solid rgba(0,102,204,0.25)',
            background:
              'radial-gradient(120% 140% at 50% 30%, #1a2740 0%, #111a2b 45%, #0a0f1a 100%)',
            boxShadow: '0 30px 80px -30px rgba(0,102,204,0.35)',
            overflow: 'hidden',
          }}
        >
          {/* LEFT: ungoverned tools */}
          <div style={sideColStyle('left')}>
            <span style={clusterLabel}>Sin gobierno</span>
            {['Claude Code', 'Cursor', 'Copilot'].map((t) => (
              <span key={t} style={dimChip}>
                {t}
              </span>
            ))}
          </div>

          {/* RIGHT: NexusMind control plane */}
          <div style={sideColStyle('right')}>
            <span style={{ ...clusterLabel, color: 'var(--color-accent-blue)' }}>
              NexusMind
            </span>
            <div
              style={{
                width: 132,
                borderRadius: 14,
                padding: '12px 10px',
                textAlign: 'center',
                border: `1px solid ${complete ? 'rgba(48,209,88,0.5)' : 'rgba(0,102,204,0.35)'}`,
                background: complete ? 'rgba(48,209,88,0.08)' : 'rgba(0,102,204,0.10)',
                transition: 'all 0.4s ease',
              }}
            >
              <div
                style={{
                  fontSize: 12,
                  fontWeight: 700,
                  color: 'var(--color-text-primary)',
                  marginBottom: 8,
                }}
              >
                Control Plane
              </div>
              {/* progress bar */}
              <div
                style={{
                  height: 6,
                  borderRadius: 999,
                  background: 'rgba(255,255,255,0.08)',
                  overflow: 'hidden',
                }}
              >
                <div
                  style={{
                    height: '100%',
                    width: `${progress}%`,
                    borderRadius: 999,
                    background: complete
                      ? 'var(--color-status-success)'
                      : 'var(--color-accent-blue)',
                    transition: 'width 0.2s linear, background 0.4s ease',
                  }}
                />
              </div>
              <div
                style={{
                  fontSize: 11,
                  marginTop: 6,
                  color: complete
                    ? 'var(--color-status-success)'
                    : 'var(--color-text-secondary)',
                }}
              >
                {complete ? '✓ Gobernado' : `${progress}%`}
              </div>
            </div>
          </div>

          {/* ROBOT */}
          <div
            style={{
              position: 'absolute',
              bottom: 46,
              left: `${phase.x}%`,
              transform: 'translateX(-50%)',
              transition: 'left 0.25s ease-out',
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              zIndex: 3,
            }}
          >
            <div style={spriteStyle} />
            {/* ground shadow */}
            <div
              style={{
                width: 90,
                height: 12,
                marginTop: -6,
                borderRadius: '50%',
                background:
                  'radial-gradient(ellipse, rgba(0,0,0,0.45) 0%, transparent 70%)',
              }}
            />
          </div>

          {/* status line */}
          <div
            style={{
              position: 'absolute',
              bottom: 16,
              left: 0,
              right: 0,
              textAlign: 'center',
              fontSize: 13,
              color: 'var(--color-text-secondary)',
              zIndex: 4,
            }}
          >
            {phase.status}
          </div>
        </div>

        {/* scroll hint */}
        <p
          className="mt-6 text-sm"
          style={{ color: 'var(--color-text-tertiary)' }}
        >
          {complete ? 'Tu conocimiento, bajo control.' : 'Desplázate para ver la migración ↓'}
        </p>
      </div>
    </section>
  );
}

const clusterLabel: React.CSSProperties = {
  fontSize: 10,
  letterSpacing: '0.08em',
  textTransform: 'uppercase',
  color: 'var(--color-text-tertiary)',
  fontWeight: 700,
};

const dimChip: React.CSSProperties = {
  fontSize: 11,
  padding: '4px 10px',
  borderRadius: 999,
  color: 'var(--color-text-secondary)',
  background: 'rgba(255,255,255,0.04)',
  border: '1px solid rgba(255,255,255,0.08)',
  opacity: 0.6,
};

function sideColStyle(side: 'left' | 'right'): React.CSSProperties {
  return {
    position: 'absolute',
    top: 28,
    [side]: 20,
    display: 'flex',
    flexDirection: 'column',
    gap: 6,
    alignItems: side === 'left' ? 'flex-start' : 'flex-end',
    zIndex: 1,
  };
}
