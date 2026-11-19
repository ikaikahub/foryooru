// Gelbooru API relay for the web version of Foryooru, deployed as a Vercel function.
// Gelbooru's API sends no CORS headers, and it answers every request from Cloudflare Workers with 429,
// so API calls come here while images still go through the Worker in proxy/.

const SITE = 'https://gelbooru.com';
// only pages on these origins may use the relay
const ALLOWED_ORIGINS = ['http://localhost:5173', 'http://localhost:5190', 'https://ikaikahub.github.io'];
// browsers can't set a Cookie header themselves, so the app sends the user's Gelbooru login cookie in this one
const COOKIE_HEADER = 'X-Gelbooru-Cookie';

// The shared key lets a Gelbooru link open while the recipient is on Danbooru.
// It is only added to plain one-post lookups, so it can't be used for searches.
const SHARED_KEY = process.env.GELBOORU_API_KEY;
const SHARED_USER = process.env.GELBOORU_USER_ID;
const LOOKUP_LIMIT = 20;
const LOOKUP_WINDOW_MS = 60_000;
const LOOKUP_PARAMS = new Set(['page', 's', 'q', 'json', 'tags', 'pid', 'limit']);
const lookups = new Map();

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

// the shared key only covers `tags=id:<n>&limit=1`, anything else needs the caller's own key
function singlePostId(searchParams) {
  if (searchParams.get('page') !== 'dapi' || searchParams.get('s') !== 'post') return null;
  if (searchParams.get('q') !== 'index' || searchParams.get('limit') !== '1') return null;
  const id = searchParams.get('tags')?.match(/^id:(\d+)$/)?.[1];
  if (!id) return null;
  for (const name of searchParams.keys()) if (!LOOKUP_PARAMS.has(name)) return null;
  return id;
}

function tooManyLookups(ip) {
  const now = Date.now();
  const seen = lookups.get(ip);
  if (!seen || now - seen.start > LOOKUP_WINDOW_MS) {
    if (lookups.size > 5000) lookups.clear();
    lookups.set(ip, { start: now, count: 1 });
    return false;
  }
  seen.count += 1;
  return seen.count > LOOKUP_LIMIT;
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
    let forward;
    let cache;

    if (searchParams.get('page') === 'dapi' && !searchParams.get('api_key')) {
      const id = singlePostId(searchParams);
      if (!id) return new Response('Add your own Gelbooru API key for this request', { status: 403, headers: cors });
      if (!SHARED_KEY || !SHARED_USER) return new Response('Gelbooru lookups are not configured', { status: 503, headers: cors });
      const ip = request.headers.get('x-forwarded-for')?.split(',')[0].trim() ?? 'unknown';
      if (tooManyLookups(ip)) return new Response('Too many Gelbooru lookups, try again in a minute', { status: 429, headers: cors });
      const params = new URLSearchParams({
        page: 'dapi',
        s: 'post',
        q: 'index',
        json: '1',
        tags: `id:${id}`,
        pid: '0',
        limit: '1',
        api_key: SHARED_KEY,
        user_id: SHARED_USER,
      });
      forward = { url: `${SITE}/index.php?${params}`, needsCookie: false };
      // the same shared link is opened by many people, so let the edge answer repeats
      cache = 'public, s-maxage=86400';
    } else {
      forward = target(searchParams, search);
    }

    if (!forward) return new Response('Only the Gelbooru API and favourites are relayed', { status: 400, headers: cors });
    const cookie = request.headers.get(COOKIE_HEADER);
    if (forward.needsCookie && !cookie) return new Response('Missing the Gelbooru login cookie', { status: 401, headers: cors });

    const headers = { 'User-Agent': 'foryooru-web' };
    if (forward.needsCookie) headers.Cookie = cookie;
    const response = await fetch(forward.url, { headers });
    return new Response(response.body, {
      status: response.status,
      headers: {
        ...cors,
        'Content-Type': response.headers.get('Content-Type') ?? 'application/json',
        ...(cache && { 'Cache-Control': cache }),
      },
    });
  },
};
