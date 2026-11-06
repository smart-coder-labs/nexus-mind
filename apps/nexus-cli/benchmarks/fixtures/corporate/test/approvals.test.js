import test from 'node:test';
import assert from 'node:assert/strict';
import { ApprovalService } from '../src/approvals.js';

const sample = { id: 'r1', requesterId: 'alice', amount: 250, reason: 'Monitor' };
const manager = { actorId: 'bob', role: 'manager', idempotencyKey: 'k1' };

test('submission creates a pending immutable snapshot', () => {
  const service = new ApprovalService(() => '2026-01-01T00:00:00Z');
  const result = service.submit(sample);
  assert.equal(result.status, 'pending');
  result.status = 'tampered';
  assert.equal(service.get('r1').status, 'pending');
});
test('manager can approve and audit records who/when', () => {
  const service = new ApprovalService(() => '2026-01-01T00:00:00Z');
  service.submit(sample);
  assert.equal(service.decide('r1', { ...manager, action: 'approve' }).status, 'approved');
  assert.deepEqual(service.history('r1').map(({ actorId, action, at }) => [actorId, action, at]),
    [['bob', 'approve', '2026-01-01T00:00:00Z']]);
});
test('requester cannot approve their own request', () => {
  const service = new ApprovalService(); service.submit(sample);
  assert.throws(() => service.decide('r1', { actorId: 'alice', role: 'manager', action: 'approve', idempotencyKey: 'k' }), /self|own|mismo|propia/i);
  assert.equal(service.get('r1').status, 'pending');
});
test('non-manager cannot decide', () => {
  const service = new ApprovalService(); service.submit(sample);
  assert.throws(() => service.decide('r1', { actorId: 'eve', role: 'requester', action: 'approve', idempotencyKey: 'k' }));
});
test('idempotent retry does not duplicate audit events', () => {
  const service = new ApprovalService(); service.submit(sample);
  const input = { ...manager, action: 'reject' };
  service.decide('r1', input); service.decide('r1', input);
  assert.equal(service.history('r1').length, 1);
  assert.equal(service.get('r1').status, 'rejected');
});
test('invalid amount, duplicate id and second decision are rejected', () => {
  const service = new ApprovalService();
  assert.throws(() => service.submit({ ...sample, amount: -1 }));
  service.submit(sample);
  assert.throws(() => service.submit(sample));
  service.decide('r1', { ...manager, action: 'approve' });
  assert.throws(() => service.decide('r1', { ...manager, action: 'reject', idempotencyKey: 'k2' }));
});
