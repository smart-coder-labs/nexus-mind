const snapshot = (value) => structuredClone(value);

export class ApprovalService {
  constructor(clock = () => new Date().toISOString()) {
    this.clock = clock;
    this.requests = new Map();
    this.audit = [];
    this.idempotentDecisions = new Map();
  }

  submit({ id, requesterId, amount, reason }) {
    if (!Number.isFinite(amount) || amount <= 0) {
      throw new Error('amount must be a positive finite number');
    }
    if (this.requests.has(id)) {
      throw new Error('duplicate request id');
    }

    const request = { id, requesterId, amount, reason, status: 'pending' };
    this.requests.set(id, snapshot(request));
    this.idempotentDecisions.set(id, new Map());
    return snapshot(request);
  }

  decide(id, { actorId, role, action, idempotencyKey }) {
    const request = this.requests.get(id);
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

    const decisions = this.idempotentDecisions.get(id);
    // Check an explicit retry key before state validation: a retry is valid
    // after the original request has left pending.
    if (idempotencyKey !== undefined && decisions.has(idempotencyKey)) {
      const prior = decisions.get(idempotencyKey);
      if (prior.actorId !== actorId || prior.action !== action) {
        throw new Error('idempotency key already used for another decision');
      }
      return snapshot(prior.result);
    }
    if (request.status !== 'pending') {
      throw new Error('request is no longer pending');
    }

    request.status = action === 'approve' ? 'approved' : 'rejected';
    const event = { requestId: id, actorId, action, at: this.clock() };
    this.audit.push(snapshot(event));

    const result = snapshot(request);
    if (idempotencyKey !== undefined) {
      decisions.set(idempotencyKey, { actorId, action, result: snapshot(result) });
    }
    return result;
  }

  get(id) {
    const request = this.requests.get(id);
    return request === undefined ? undefined : snapshot(request);
  }

  history(id) {
    return this.audit
      .filter((event) => event.requestId === id)
      .map((event) => snapshot(event));
  }
}
