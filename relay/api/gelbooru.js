// Gelbooru API relay for the web version of Foryooru, deployed as a Vercel function.
// Gelbooru's API sends no CORS headers, and it answers every request from Cloudflare Workers with 429,
// so API calls come here while images still go through the Worker in proxy/.

const SITE = 'https://gelbooru.com';
// only pages on these origins may use the relay
const ALLOWED_ORIGINS = ['http://localhost:5190', 'https://ikaikahub.github.io'];
// browsers can't set a Cookie header themselves, so the app sends the user's Gelbooru login cookie in this one
const COOKIE_HEADER = 'X-Gelbooru-Cookie';

// what the relay forwards: the API, plus adding and removing favourites, which need the login cookie
function target(searchParams, search) {
  const page = searchParams.get('page');
  if (page === 'dapi') return { url: `${SITE}/index.php${search}`, needsCookie: false };
  if (page === 'favorites' && searchParams.get('s') === 'delete') return { url: `${SITE}/index.php${search}`, needsCookie: true };
  if (page === 'addfav') {
    return { url: `${SITE}/public/addfav.php?id=${encodeURIComponent(searchParams.get('id') ?? '')}`, needsCookie: true };
  }
  return null;
}

export default {
  async fetch(request) {
    const origin = request.headers.get('Origin');
    if (!origin || !ALLOWED_ORIGINS.includes(origin)) return new Response('Forbidden', { status: 403 });
    const cors = { 'Access-Control-Allow-Origin': origin, Vary: 'Origin' };

    if (request.method === 'OPTIONS') {
      return new Response(null, {
        status: 204,
        headers: {
          ...cors,
          'Access-Control-Allow-Methods': 'GET',
          'Access-Control-Allow-Headers': COOKIE_HEADER,
          'Access-Control-Max-Age': '86400',
        },
      });
    }
    if (request.method !== 'GET') return new Response('Method not allowed', { status: 405, headers: cors });

    const { search, searchParams } = new URL(request.url);
    const forward = target(searchParams, search);
    if (!forward) return new Response('Only the Gelbooru API and favourites are relayed', { status: 400, headers: cors });
    const cookie = request.headers.get(COOKIE_HEADER);
    if (forward.needsCookie && !cookie) return new Response('Missing the Gelbooru login cookie', { status: 401, headers: cors });

    const headers = { 'User-Agent': 'foryooru-web' };
    if (forward.needsCookie) headers.Cookie = cookie;
    const response = await fetch(forward.url, { headers });
    return new Response(response.body, {
      status: response.status,
      headers: { ...cors, 'Content-Type': response.headers.get('Content-Type') ?? 'application/json' },
    });
  },
};
