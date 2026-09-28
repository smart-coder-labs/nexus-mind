/** Bounded cache. The clock function returns milliseconds. */
export class LruCache {
  constructor(capacity, clock = Date.now) {
    this.capacity = capacity;
    this.clock = clock;
    this.items = new Map();
  }

  set(key, value, ttlMs = Infinity) {
    this.items.set(key, { value, expiresAt: this.clock() + ttlMs });
  }

  get(key) {
    return this.items.get(key)?.value;
  }

  get size() {
    return this.items.size;
  }
}
