// Gelbooru image proxy for the web version of Foryooru, deployed as a Cloudflare Worker.
// Gelbooru's image servers redirect any request without a gelbooru.com referer, so this adds one,
// only for pages on the origins listed in ALLOWED_ORIGINS (wrangler.toml).
// API calls go through relay/ instead, since Gelbooru answers Cloudflare Workers with 429.

const IMAGE_HOST = /^(img|video-cdn)\d*\.gelbooru\.com$/;
const REFERER = 'https://gelbooru.com/';
const IMAGE_CACHE_SECONDS = 60 * 60 * 24 * 30;

function allowedOrigins(env) {
  return (env.ALLOWED_ORIGINS ?? '')
    .split(',')
    .map((origin) => origin.trim())
    .filter(Boolean);
}

// fetch() sends an Origin header but plain <img> loads only send a Referer, so either one has to match
function callerOrigin(request) {
  const origin = request.headers.get('Origin');
  if (origin) return origin;
  const referer = request.headers.get('Referer');
  try {
    return referer ? new URL(referer).origin : null;
  } catch {
    return null;
  }
}

function withCors(response, origin, extra = {}) {
  const headers = new Headers(response.headers);
  headers.set('Access-Control-Allow-Origin', origin);
  headers.set('Vary', 'Origin');
  for (const [name, value] of Object.entries(extra)) headers.set(name, value);
  return new Response(response.body, { status: response.status, headers });
}

async function forwardImage(request, url) {
  // /img/<host>/<path on that host>
  const [, , host, ...rest] = url.pathname.split('/');
  if (!IMAGE_HOST.test(host ?? '')) return new Response('Not a Gelbooru image host', { status: 400 });
  const headers = { Referer: REFERER };
  // video players ask for byte ranges, and dropping the header makes the origin return the whole file
  const range = request.headers.get('Range');
  if (range) headers.Range = range;
  return fetch(`https://${host}/${rest.join('/')}`, {
    method: request.method,
    headers,
    // Cloudflare keeps images at its edge, so repeat views don't reach Gelbooru again
    cf: { cacheTtl: IMAGE_CACHE_SECONDS, cacheEverything: true },
  });
}

export default {
  async fetch(request, env) {
    const origin = callerOrigin(request);
    if (!origin || !allowedOrigins(env).includes(origin)) return new Response('Forbidden', { status: 403 });

    if (request.method === 'OPTIONS') {
      return new Response(null, {
        status: 204,
        headers: {
          'Access-Control-Allow-Origin': origin,
          'Access-Control-Allow-Methods': 'GET, HEAD',
          'Access-Control-Max-Age': '86400',
          Vary: 'Origin',
        },
      });
    }
    if (request.method !== 'GET' && request.method !== 'HEAD') return new Response('Method not allowed', { status: 405 });

    const url = new URL(request.url);
    if (url.pathname.startsWith('/img/')) {
      return withCors(await forwardImage(request, url), origin, { 'Cache-Control': `public, max-age=${IMAGE_CACHE_SECONDS}` });
    }
    return new Response('Not found', { status: 404 });
  },
};
