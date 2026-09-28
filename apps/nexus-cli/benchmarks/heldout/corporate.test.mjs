import test from 'node:test';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

const { ApprovalService } = await import(pathToFileURL(process.env.BENCH_IMPL).href);
const request = { id: 'r', requesterId: 'alice', amount: 100, reason: 'Monitor' };

test('a rejected authorization leaves state and audit unchanged', () => {
  const service = new ApprovalService(); service.submit(request);
  assert.throws(() => service.decide('r', { actorId: 'eve', role: 'requester', action: 'approve', idempotencyKey: 'x' }));
  assert.equal(service.get('r').status, 'pending');
  assert.deepEqual(service.history('r'), []);
});
test('returned get and history values cannot mutate internal state', () => {
  const service = new ApprovalService(); service.submit(request);
  const item = service.get('r'); item.status = 'approved';
  assert.equal(service.get('r').status, 'pending');
  service.decide('r', { actorId: 'bob', role: 'manager', action: 'approve', idempotencyKey: 'k' });
  const events = service.history('r'); events[0].action = 'reject';
  assert.equal(service.history('r')[0].action, 'approve');
});
test('invalid action is rejected without a state transition', () => {
  const service = new ApprovalService(); service.submit(request);
  assert.throws(() => service.decide('r', { actorId: 'bob', role: 'manager', action: 'delete', idempotencyKey: 'k' }));
  assert.equal(service.get('r').status, 'pending');
});
test('nonfinite amount is rejected', () => {
  const service = new ApprovalService();
  assert.throws(() => service.submit({ ...request, amount: Infinity }));
});
