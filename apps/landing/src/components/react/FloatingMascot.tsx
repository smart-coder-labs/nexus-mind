import { useEffect, useRef, useState } from 'react';

/**
 * FloatingMascot — a transparent robot that lives fixed over the landing and
 * roams the whole page: it walks along the bottom edge as the user scrolls
 * (x tied to scroll progress), plays its walk cycle while moving, idles when
 * the user stops, and faces its travel direction.
 *
 * Frames are real alpha-transparent PNGs under /robot/frames/<state>/, described
 * by /robot/frames/manifest.json. If the manifest is absent the island renders
 * nothing (keeps the page clean before assets exist).
 *
 * The source art faces LEFT; we flip horizontally to face right.
 */

type StateCfg = { frames: number; fps: number };
type Manifest = {
  displayHeight?: number;
  canvas: { w: number; h: number };
  states: Record<string, StateCfg>;
};

const BASE = '/robot/frames';
const FLOAT_H = 170; // on-screen robot height in px
const X_MIN = 0.08; // travel band across the viewport (fractions of vw)
const X_MAX = 0.88;

function framePath(state: string, i: number) {
  return `${BASE}/${state}/${state}-${String(i).padStart(2, '0')}.png`;
}

export default function FloatingMascot() {
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [state, setState] = useState<'idle' | 'walk'>('idle');
  const [frame, setFrame] = useState(0);
  const [x, setX] = useState(X_MIN); // fraction of viewport width (robot centre)
  const [facingRight, setFacingRight] = useState(true);

  const reduced = useRef(false);
  const lastScroll = useRef(0);
  const idleTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  // Load manifest + preload every frame so cycling never flickers.
  useEffect(() => {
    reduced.current = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    let alive = true;
    fetch(`${BASE}/manifest.json`)
      .then((r) => (r.ok ? r.json() : null))
      .then((m: Manifest | null) => {
        if (!alive || !m) return;
        setManifest(m);
        Object.entries(m.states).forEach(([s, cfg]) => {
          for (let i = 1; i <= cfg.frames; i++) {
            const im = new Image();
            im.src = framePath(s, i);
          }
        });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  // Scroll → horizontal position, walk/idle state, and facing direction.
  useEffect(() => {
    if (!manifest || reduced.current) return;
    const onScroll = () => {
      const st = window.scrollY;
      const max = document.documentElement.scrollHeight - window.innerHeight;
      const p = max > 0 ? Math.min(1, Math.max(0, st / max)) : 0;
      setX(X_MIN + p * (X_MAX - X_MIN));

      const delta = st - lastScroll.current;
      if (Math.abs(delta) > 1) {
        setState('walk');
        setFacingRight(delta > 0); // scroll down → travel right
      }
      lastScroll.current = st;

      clearTimeout(idleTimer.current);
      idleTimer.current = setTimeout(() => setState('idle'), 320);
    };
    window.addEventListener('scroll', onScroll, { passive: true });
    onScroll();
    return () => {
      window.removeEventListener('scroll', onScroll);
      clearTimeout(idleTimer.current);
    };
  }, [manifest]);

  // Frame cycling at the active state's fps.
  useEffect(() => {
    if (!manifest || reduced.current) return;
    const cfg = manifest.states[state];
    if (!cfg) return;
    setFrame(0);
    let raf = 0;
    let last = 0;
    const interval = 1000 / cfg.fps;
    const tick = (t: number) => {
      if (t - last >= interval) {
        last = t;
        setFrame((f) => (f + 1) % cfg.frames);
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [manifest, state]);

  if (!manifest) return null;
  const cfg = manifest.states[state] ?? manifest.states.idle;
  const idx = (frame % cfg.frames) + 1;
  const aspect = manifest.canvas.w / manifest.canvas.h;
  const w = FLOAT_H * aspect;

  return (
    <img
      src={framePath(state, idx)}
      alt=""
      aria-hidden="true"
      style={{
        position: 'fixed',
        bottom: 8,
        left: `calc(${(x * 100).toFixed(2)}vw - ${w / 2}px)`,
        width: w,
        height: FLOAT_H,
        pointerEvents: 'none',
        zIndex: 40,
        // Art faces left → flip to face right when travelling right.
        transform: facingRight ? 'scaleX(-1)' : 'scaleX(1)',
        transition: 'left 0.12s linear',
        filter: 'drop-shadow(0 10px 16px rgba(0,0,0,0.5))',
        willChange: 'left, transform',
      }}
    />
  );
}
