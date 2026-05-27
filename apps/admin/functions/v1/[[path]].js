const BACKEND = 'https://nexusmind-backend.fly.dev';

export async function onRequest({ request }) {
  const url = new URL(request.url);
  return fetch(BACKEND + url.pathname + url.search, request);
}
