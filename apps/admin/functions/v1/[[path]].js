const BACKEND = 'https://nexusmind-backend.fly.dev';

export async function onRequest(context) {
  const { request } = context;
  const url = new URL(request.url);
  const target = `${BACKEND}${url.pathname}${url.search}`;

  const proxyRequest = new Request(target, {
    method: request.method,
    headers: request.headers,
    body: request.method !== 'GET' && request.method !== 'HEAD' ? request.body : undefined,
    redirect: 'follow',
  });

  return fetch(proxyRequest);
}
