import { createServer as createHttpServer } from 'node:http';
import { randomUUID } from 'node:crypto';

const MAX_BODY_SIZE = 64 * 1024;
const JSON_CONTENT_TYPE = 'application/json; charset=utf-8';
const SKU_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

function sendJson(res, status, body) {
  const encoded = JSON.stringify(body);
  res.writeHead(status, {
    'content-type': JSON_CONTENT_TYPE,
    'content-length': Buffer.byteLength(encoded),
  });
  res.end(encoded);
}

function sendError(res, status, error) {
  sendJson(res, status, { error });
}

function isJsonContentType(contentType) {
  if (typeof contentType !== 'string') return false;
  return contentType.split(';', 1)[0].trim().toLowerCase() === 'application/json';
}

function isValidOrder(value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const keys = Object.keys(value);
  if (keys.length !== 2 || !keys.includes('sku') || !keys.includes('quantity')) return false;

  return typeof value.sku === 'string'
    && value.sku.length <= 128
    && SKU_PATTERN.test(value.sku)
    && Number.isSafeInteger(value.quantity)
    && value.quantity > 0;
}

async function readJsonBody(req) {
  const declaredLength = req.headers['content-length'];
  if (declaredLength !== undefined) {
    if (!/^\d+$/.test(declaredLength) || Number(declaredLength) > MAX_BODY_SIZE) {
      return { tooLarge: true };
    }
  }

  const chunks = [];
  let size = 0;
  try {
    for await (const chunk of req) {
      size += chunk.length;
      if (size > MAX_BODY_SIZE) return { tooLarge: true };
      chunks.push(chunk);
    }
  } catch {
    return { invalid: true };
  }

  try {
    return { value: JSON.parse(Buffer.concat(chunks).toString('utf8')) };
  } catch {
    return { invalid: true };
  }
}

export function createServer() {
  const orders = new Map();
  const idempotencyKeys = new Map();

  return createHttpServer(async (req, res) => {
    const url = new URL(req.url, 'http://localhost');

    if (req.method === 'GET' && url.pathname === '/health') {
      sendJson(res, 200, { status: 'ok' });
      return;
    }

    if (req.method === 'GET' && url.pathname.startsWith('/orders/')) {
      const id = url.pathname.slice('/orders/'.length);
      const order = orders.get(id);
      if (!id || !order) sendError(res, 404, 'order not found');
      else sendJson(res, 200, order);
      return;
    }

    if (req.method !== 'POST' || url.pathname !== '/orders') {
      sendError(res, 404, 'route not found');
      return;
    }

    if (!isJsonContentType(req.headers['content-type'])) {
      sendError(res, 415, 'content-type must be application/json');
      return;
    }

    const parsed = await readJsonBody(req);
    if (parsed.tooLarge) {
      sendError(res, 413, 'request body too large');
      return;
    }
    if (parsed.invalid || !isValidOrder(parsed.value)) {
      sendError(res, 400, 'invalid order payload');
      return;
    }

    const payload = { sku: parsed.value.sku, quantity: parsed.value.quantity };
    const key = req.headers['idempotency-key'];
    if (key !== undefined) {
      const previous = idempotencyKeys.get(key);
      if (previous) {
        if (previous.sku !== payload.sku || previous.quantity !== payload.quantity) {
          sendError(res, 409, 'idempotency key conflicts with a different payload');
        } else {
          sendJson(res, 200, previous.order);
        }
        return;
      }
    }

    const order = { id: randomUUID(), ...payload };
    orders.set(order.id, order);
    if (key !== undefined) idempotencyKeys.set(key, { ...payload, order });
    sendJson(res, 201, order);
  });
}
