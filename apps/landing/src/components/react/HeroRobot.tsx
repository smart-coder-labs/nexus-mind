import { useEffect, useState } from 'react';
import { motion } from 'framer-motion';

/**
 * HeroRobot — the mascot's cinematic entrance in the hero. Cycles the idle
 * frames (real alpha PNGs under /robot/frames/idle), fades + scales in, and
 * floats gently. Respects prefers-reduced-motion (static first frame).
 */

const BASE = '/robot/frames';

export default function HeroRobot({ height = 220 }: { height?: number }) {
  const [frames, setFrames] = useState<string[]>([]);
  const [i, setI] = useState(0);
  const [aspect, setAspect] = useState(384 / 544);

  useEffect(() => {
    let alive = true;
    fetch(`${BASE}/manifest.json`)
      .then((r) => (r.ok ? r.json() : null))
      .then((m) => {
        if (!alive || !m?.states?.idle) return;
        const n = m.states.idle.frames as number;
        setAspect(m.canvas.w / m.canvas.h);
        const fr = Array.from(
          { length: n },
          (_, k) => `${BASE}/idle/idle-${String(k + 1).padStart(2, '0')}.png`,
        );
        fr.forEach((s) => {
          const im = new Image();
          im.src = s;
        });
        setFrames(fr);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!frames.length) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    let raf = 0;
    let last = 0;
    const fps = 5;
    const tick = (t: number) => {
      if (t - last > 1000 / fps) {
        last = t;
        setI((v) => (v + 1) % frames.length);
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [frames]);

  if (!frames.length) return null;
  const w = height * aspect;

  return (
    <motion.img
      src={frames[i]}
      alt=""
      aria-hidden="true"
      initial={{ opacity: 0, scale: 0.85 }}
      animate={{ opacity: 1, scale: 1, y: [0, -12, 0] }}
      transition={{
        opacity: { duration: 0.8, ease: 'easeOut' },
        scale: { duration: 0.8, ease: 'easeOut' },
        y: { duration: 4.5, repeat: Infinity, ease: 'easeInOut' },
      }}
      style={{
        width: w,
        height,
        filter: 'drop-shadow(0 22px 44px rgba(0,102,204,0.5))',
      }}
    />
  );
}
