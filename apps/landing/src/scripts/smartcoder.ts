import { initHeroMesh } from './hero-mesh';
import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
gsap.registerPlugin(ScrollTrigger);
initHeroMesh();

const themeButton=document.querySelector<HTMLButtonElement>('#sc-theme');
const syncTheme=()=>themeButton?.setAttribute('aria-pressed',String(document.documentElement.dataset.theme==='dark'));
const toggleTheme=()=>{const next=document.documentElement.dataset.theme==='dark'?'light':'dark';document.documentElement.dataset.theme=next;syncTheme();try{localStorage.setItem('nexusmind-comparison-theme',next);}catch{}};
syncTheme();themeButton?.addEventListener('click',toggleTheme);
const menu=document.querySelector<HTMLDetailsElement>('.sc-menu');
const closeMenu=(event:Event)=>{if(menu && !menu.contains(event.target as Node))menu.open=false;};
const escapeMenu=(event:KeyboardEvent)=>{if(event.key==='Escape'&&menu?.open){menu.open=false;menu.querySelector('summary')?.focus();}};
const menuLink=(event:Event)=>{if((event.target as Element).closest('a')&&menu)menu.open=false;};
document.addEventListener('click',closeMenu);document.addEventListener('keydown',escapeMenu);menu?.addEventListener('click',menuLink);

const media=gsap.matchMedia();
media.add({motion:'(prefers-reduced-motion: no-preference)',desktop:'(min-width: 801px)'},context=>{
  if(!context.conditions?.motion)return;
  const desktop=!!context.conditions.desktop;
  gsap.from('.sc-hero-copy',{opacity:.45,y:24,duration:1,ease:'expo.out'});
  gsap.from('.sc-hero-visual',{opacity:.45,y:36,duration:1.1,ease:'expo.out',delay:.12});
  if(desktop) gsap.timeline({scrollTrigger:{trigger:'.sc-hero',start:'top top',end:'bottom top',scrub:.7}})
    .to('.sc-context-back',{y:50,rotation:0,ease:'none'},0).to('.sc-context-front',{y:-36,rotation:0,ease:'none'},0);
  document.querySelectorAll<HTMLElement>('[data-sc-chapter]').forEach(section=>{
    const copy=section.querySelector('[data-sc-copy]');
    const visual=section.querySelector('[data-sc-visual]');
    if(copy)gsap.from(copy,{y:28,opacity:.45,duration:.9,ease:'expo.out',scrollTrigger:{trigger:copy,start:'top 93%',once:true}});
    if(visual)gsap.from(visual,{y:desktop?48:20,rotation:0,opacity:.5,duration:.9,ease:'power3.out',scrollTrigger:{trigger:visual,start:'top 95%',end:'top 62%',scrub:desktop?.6:false,once:true}});
    section.querySelectorAll('[data-sc-item]').forEach((item,i)=>{
      gsap.from(item,{y:desktop?45+(i%3)*15:20,opacity:.5,duration:.8,ease:'power3.out',scrollTrigger:{trigger:item,start:'top 95%',end:'top 68%',scrub:desktop?.6:false,once:true}});
      const img=item.querySelector('img');
      if(img)gsap.from(img,{scale:1.12,duration:1.2,ease:'power2.out',scrollTrigger:{trigger:img,start:'top 95%',end:'bottom 55%',scrub:desktop?.7:false,once:true}});
    });
  });
  if(desktop) gsap.timeline({scrollTrigger:{trigger:'.sc-architecture',start:'top 88%',end:'top 24%',scrub:.65,once:true}})
    .from('.sc-diagram-tools span',{y:-16,opacity:.45,stagger:.05,duration:.35},0)
    .from('.sc-connector',{scaleY:0,stagger:.2,duration:.25},.15)
    .from('.sc-diagram-core',{scale:.92,duration:.4},.2)
    .from('.sc-diagram-models',{y:15,opacity:.4,duration:.3},.45);
  gsap.from('.sc-close-rule',{scaleX:.15,duration:1,ease:'expo.out',scrollTrigger:{trigger:'.sc-close',start:'top 85%',once:true}});
});
const refresh=()=>ScrollTrigger.refresh();
// Disclosure height changes move the later chapters; refresh their positions.
const disclosures=Array.from(document.querySelectorAll('.sc-capability-list details'));
disclosures.forEach(el=>el.addEventListener('toggle',refresh));
document.fonts.ready.then(refresh);window.addEventListener('load',refresh,{once:true});
const cleanup=()=>{media.revert();themeButton?.removeEventListener('click',toggleTheme);document.removeEventListener('click',closeMenu);document.removeEventListener('keydown',escapeMenu);menu?.removeEventListener('click',menuLink);disclosures.forEach(el=>el.removeEventListener('toggle',refresh));window.removeEventListener('load',refresh);};
document.addEventListener('astro:before-swap',cleanup,{once:true});
if(import.meta.hot)import.meta.hot.dispose(cleanup);
