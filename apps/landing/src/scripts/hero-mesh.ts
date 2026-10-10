/** A locally rendered 3D field: no external scene, video or WebGL dependency. */
export function initHeroMesh() {
  const canvas = document.querySelector<HTMLCanvasElement>('#heroMesh');
  const hero = document.querySelector<HTMLElement>('#hero');
  const toggle = document.querySelector<HTMLButtonElement>('#meshToggle');
  if (!canvas || !hero || !toggle) return;
  const ctx = canvas.getContext('2d');
  if (!ctx) { toggle.hidden = true; return; }
  const cursor = document.querySelector<HTMLElement>('#meshCursor');
  const finePointer = matchMedia('(pointer: fine)');
  const motion = matchMedia('(prefers-reduced-motion: reduce)');
  let meshColor='126,165,255';
  const readPalette=()=>{meshColor=getComputedStyle(hero).getPropertyValue('--mesh-color').trim() || '126,165,255';};
  readPalette();
  let width = 0, height = 0, frame = 0, time = 0, last = 0;
  let visible = true, paused = motion.matches;
  let pointerX = 0, pointerY = 0, tiltX = 0, tiltY = 0;
  let cursorX = 0, cursorY = 0, trailX = 0, trailY = 0;
  let presence = 0, speed = 0, pointerInside = false, lastPointerTime = 0;
  let wave: { x: number; y: number; started: number } | null = null;
  type MeshPoint = { x: number; y: number; z: number; light: number };
  const resetPointer = () => {
    pointerInside = false; pointerX = 0; pointerY = 0; lastPointerTime = 0;
    cursor?.classList.remove('is-visible', 'is-interactive');
    hero.classList.remove('pointer-ready');
  };
  const updateButton = () => {
    toggle.hidden = motion.matches;
    toggle.textContent = paused ? 'Activar fondo' : 'Pausar fondo';
    toggle.setAttribute('aria-pressed', String(paused));
  };
  function draw() {
    if (!ctx || !canvas) return;
    ctx.clearRect(0, 0, width, height);
    const interactive = !paused && !motion.matches && finePointer.matches;
    const gain = interactive && pointerInside ? 1 : 0;
    presence += (gain - presence) * .12;
    speed *= .9;
    trailX += (cursorX - trailX) * .14;
    trailY += (cursorY - trailY) * .14;
    tiltX += (pointerX - tiltX) * .08;
    tiltY += (pointerY - tiltY) * .08;
    const radius = Math.min(310, width * .26);
    const energy = presence * (.55 + speed * .7);
    const waveAge = wave ? time - wave.started : 10;
    if (waveAge > 1.8) wave = null;
    const mobile = width < 700;
    const columns = mobile ? 56 : 88, rows = mobile ? 26 : 38;
    const size = Math.min(width * .38, height * .55);
    const yaw = time * .085 + tiltX * .34;
    const pitch = -.48 + Math.sin(time * .14) * .12 + tiltY * .24;
    const points: MeshPoint[][] = [];
    for (let i = 0; i <= rows; i++) {
      const v = i / rows * Math.PI * 2;
      points[i] = [];
      for (let j = 0; j <= columns; j++) {
        const u = j / columns * Math.PI * 2;
        const tube = .47 + .1 * Math.sin(u * 3 + time * .28 + v * 2);
        const ring = 1.18 + .22 * Math.cos(u * 2 - time * .16);
        const x = (ring + tube * Math.cos(v)) * Math.cos(u);
        const y = (ring + tube * Math.cos(v)) * Math.sin(u);
        const z = tube * Math.sin(v) + .24 * Math.sin(u * 3 + time * .13);
        const xx = x * Math.cos(yaw) - z * Math.sin(yaw);
        const zz = x * Math.sin(yaw) + z * Math.cos(yaw);
        const yy = y * Math.cos(pitch) - zz * Math.sin(pitch);
        const depth = y * Math.sin(pitch) + zz * Math.cos(pitch);
        const perspective = 4 / (4 + depth);
        // A diagonal sculptural silhouette rather than a flat circular grid.
        const angle = -.34;
        let px = width * .5 + (xx*Math.cos(angle)-yy*Math.sin(angle))*size*perspective + tiltX * 30;
        let py = height*.47 + (xx*Math.sin(angle)+yy*Math.cos(angle))*size*perspective + tiltY * 20;
        const dx = trailX - px, dy = trailY - py;
        const distance = Math.hypot(dx, dy);
        const falloff = Math.exp(-distance * distance / (radius * radius));
        // Local magnetic pull bends the connected surface; the light trails the pointer.
        const attraction = falloff * energy;
        px += dx * attraction * .28;
        py += dy * attraction * .28;
        let ripple = 0;
        if (wave) {
          const wx = px - wave.x, wy = py - wave.y;
          const wd = Math.hypot(wx, wy);
          ripple = Math.exp(-Math.pow((wd - waveAge * 420) / 65, 2)) * Math.exp(-waveAge * 1.9);
          const displacement = ripple * 28 / Math.max(wd, 1);
          px += wx * displacement; py += wy * displacement;
        }
        points[i][j] = {x:px, y:py, z:depth, light:Math.min(1, attraction + ripple)};
      }
    }
    function line(a:MeshPoint, b:MeshPoint) {
      if (!ctx) return;
      const depth = Math.max(0, Math.min(1, (a.z + 1.8) / 3.6));
      const light = (a.light + b.light) * .5;
      ctx.strokeStyle = `rgba(${meshColor},${Math.min(.85,.06 + .24 * depth + light*.62)})`;
      ctx.lineWidth = .65 + light * .65;
      ctx.beginPath(); ctx.moveTo(a.x,a.y); ctx.lineTo(b.x,b.y); ctx.stroke();
    }
    for (let i=0;i<rows;i++) for(let j=0;j<columns;j++) {
      line(points[i][j], points[i][j+1]); line(points[i][j],points[i+1][j]);
    }
    // Lit vertices give the interaction a material surface, not just a cursor spotlight.
    for(let i=0;i<rows;i+=2) for(let j=0;j<columns;j+=2) {
      const p=points[i][j];
      if(p.light < .12) continue;
      ctx.beginPath(); ctx.arc(p.x,p.y,.6+p.light*1.25,0,Math.PI*2);
      ctx.fillStyle=`rgba(${meshColor},${p.light*.8})`; ctx.fill();
    }
    canvas.dataset.motion = paused || motion.matches ? 'static' : visible && !document.hidden ? 'running' : 'suspended';
  }
  function tick(now:number) {
    frame = 0;
    if (now-last > 1000/30) { time += Math.min((now-last)/1000,.05); last=now; draw(); }
    if (visible && !paused && !motion.matches && !document.hidden) frame=requestAnimationFrame(tick);
  }
  function sync() {
    cancelAnimationFrame(frame); frame=0; last=performance.now();
    draw(); updateButton();
    if (visible && !paused && !motion.matches && !document.hidden) frame=requestAnimationFrame(tick);
  }
  const resize = new ResizeObserver(() => {
    const rect=hero.getBoundingClientRect(); width=rect.width; height=rect.height;
    const dpr=Math.min(devicePixelRatio, width<700 ? 1.25 : 1.5);
    canvas.width=Math.round(width*dpr); canvas.height=Math.round(height*dpr);
    ctx.setTransform(dpr,0,0,dpr,0,0); sync();
  });
  resize.observe(hero);
  // Repaint even a paused/reduced-motion frame when the visual theme changes.
  const paletteObserver=new MutationObserver(()=>{readPalette();sync();});
  paletteObserver.observe(document.documentElement,{attributes:true,attributeFilter:['class','data-theme']});
  const observer=new IntersectionObserver(([entry])=>{visible=entry.isIntersecting;sync();});
  observer.observe(hero);
  const visibility=()=>{if(document.hidden) resetPointer();sync();};
  const preference=()=>{paused=motion.matches;resetPointer();presence=0;wave=null;sync();};
  const pointer=(e:PointerEvent)=>{
    if(e.pointerType!=='mouse' || !finePointer.matches || paused || motion.matches) return;
    const rect=hero.getBoundingClientRect();
    const x=e.clientX-rect.left, y=e.clientY-rect.top;
    const now=performance.now();
    if(pointerInside && lastPointerTime) {
      const velocity=Math.hypot(x-cursorX,y-cursorY)/Math.max(16,now-lastPointerTime);
      speed=Math.min(1.3,speed+velocity*.18);
    } else { trailX=x;trailY=y; }
    cursorX=x;cursorY=y;lastPointerTime=now;pointerInside=true;
    pointerX=(x/width-.5)*2;pointerY=(y/height-.5)*2;
    if(cursor) {
      cursor.style.transform=`translate3d(${x}px,${y}px,0)`;
      cursor.classList.add('is-visible');
      cursor.classList.toggle('is-interactive',!!(e.target as Element).closest('a,button'));
      hero.classList.add('pointer-ready');
    }
  };
  const leave=()=>resetPointer();
  const pulse=(e:PointerEvent)=>{
    if(paused || motion.matches || e.button!==0 || (e.target as Element).closest('a,button,input,textarea,select')) return;
    const rect=hero.getBoundingClientRect();
    wave={x:e.clientX-rect.left,y:e.clientY-rect.top,started:time};
    speed=Math.min(1.3,speed+.7);
  };
  const click=()=>{paused=!paused;resetPointer();presence=0;wave=null;sync();};
  hero.addEventListener('pointermove',pointer,{passive:true}); hero.addEventListener('pointerleave',leave);
  hero.addEventListener('pointerdown',pulse);
  window.addEventListener('blur',leave);
  finePointer.addEventListener('change',leave);
  toggle.addEventListener('click',click); document.addEventListener('visibilitychange',visibility);
  motion.addEventListener('change',preference);
  toggle.hidden=motion.matches; updateButton();
  const cleanup=()=>{
    cancelAnimationFrame(frame); resize.disconnect(); observer.disconnect(); paletteObserver.disconnect();
    hero.removeEventListener('pointermove',pointer); hero.removeEventListener('pointerleave',leave);
    hero.removeEventListener('pointerdown',pulse);
    window.removeEventListener('blur',leave);
    finePointer.removeEventListener('change',leave);
    resetPointer();
    toggle.removeEventListener('click',click); document.removeEventListener('visibilitychange',visibility);
    motion.removeEventListener('change',preference);
  };
  document.addEventListener('astro:before-swap',cleanup,{once:true});
  if(import.meta.hot) import.meta.hot.dispose(cleanup);
}
