export class ApprovalService {
  constructor(clock = () => new Date().toISOString()) {
    this.clock = clock;
    this.requests = new Map();
    this.audit = [];
  }

  submit({ id, requesterId, amount, reason }) {
    this.requests.set(id, { id, requesterId, amount, reason, status: 'pending' });
    return this.requests.get(id);
  }

  decide(id, { actorId, role, action, idempotencyKey }) {
    const request = this.requests.get(id);
    if (!request) throw new Error('not found');
    request.status = action === 'approve' ? 'approved' : 'rejected';
    return request;
  }

  get(id) { return this.requests.get(id); }
  history(id) { return this.audit.filter((event) => event.requestId === id); }
}
