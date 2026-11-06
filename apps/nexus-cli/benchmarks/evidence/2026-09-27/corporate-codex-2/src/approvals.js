// State never crosses the service boundary by reference. Requests may grow
// extra fields over time, so use a deep copy instead of a shallow spread.
const snapshot = (value) => structuredClone(value);

export class ApprovalService {
  #requests = new Map();
  #audit = [];
  #decisionsByKey = new Map();

  constructor(clock = () => new Date().toISOString()) {
    this.clock = clock;
  }

  submit({ id, requesterId, amount, reason }) {
    if (!Number.isFinite(amount) || amount <= 0) {
      throw new Error('amount must be a positive finite number');
    }
    if (this.#requests.has(id)) {
      throw new Error('duplicate request id');
    }

    const request = snapshot({ id, requesterId, amount, reason, status: 'pending' });
    this.#requests.set(id, request);
    return snapshot(request);
  }

  decide(id, { actorId, role, action, idempotencyKey }) {
    const request = this.#requests.get(id);
    if (!request) throw new Error('not found');
    if (role !== 'manager' && role !== 'admin') {
      throw new Error('only a manager or admin can decide');
    }
    if (actorId === request.requesterId) {
      throw new Error('requester cannot decide their own request');
    }
    if (action !== 'approve' && action !== 'reject') {
      throw new Error('invalid decision action');
    }

    // A key is scoped to one purchase request. Check it before the state
    // transition so an already-completed retry remains safe and event-free.
    const hasIdempotencyKey = idempotencyKey !== undefined && idempotencyKey !== null;
    const requestKeys = this.#decisionsByKey.get(id);
    if (hasIdempotencyKey && requestKeys?.has(idempotencyKey)) {
      return snapshot(request);
    }
    if (request.status !== 'pending') {
      throw new Error('request is no longer pending');
    }

    request.status = action === 'approve' ? 'approved' : 'rejected';
    const event = { requestId: id, actorId, action, at: this.clock() };
    this.#audit.push(event);

    if (hasIdempotencyKey) {
      const keys = requestKeys ?? new Map();
      keys.set(idempotencyKey, true);
      this.#decisionsByKey.set(id, keys);
    }
    return snapshot(request);
  }

  get(id) {
    const request = this.#requests.get(id);
    return request === undefined ? undefined : snapshot(request);
  }

  history(id) {
    return snapshot(this.#audit.filter((event) => event.requestId === id));
  }
}
