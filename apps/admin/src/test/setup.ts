import '@testing-library/jest-dom'

// jsdom has no layout engine. Supply deterministic chart dimensions for
// Recharts interaction/data tests; this does not replace browser visual QA.
globalThis.ResizeObserver ??= class implements ResizeObserver {
  constructor(private callback: ResizeObserverCallback) {}
  observe(target: Element) {
    const contentRect = { width: 640, height: 268, top: 0, left: 0, right: 640, bottom: 268, x: 0, y: 0, toJSON() {} }
    this.callback([{ target, contentRect } as ResizeObserverEntry], this)
  }
  unobserve() {}
  disconnect() {}
}

// Radix calls these browser layout/pointer APIs; jsdom has no layout engine.
HTMLElement.prototype.scrollIntoView ??= () => {};
HTMLElement.prototype.hasPointerCapture ??= () => false;
HTMLElement.prototype.setPointerCapture ??= () => {};
HTMLElement.prototype.releasePointerCapture ??= () => {};
window.matchMedia ??= (query: string) => ({
  matches: false, media: query, onchange: null,
  addListener: () => {}, removeListener: () => {},
  addEventListener: () => {}, removeEventListener: () => {},
  dispatchEvent: () => false,
});
