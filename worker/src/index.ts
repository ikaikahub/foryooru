interface Env {
  DB: D1Database;
}

interface Item {
  list: string;
  key: string;
  present: boolean;
  sortAt: number;
  changedAt: number;
  post: unknown;
}

interface ItemRow {
  list: string;
  key: string;
  present: number;
  sort_at: number;
  changed_at: number;
  version: number;
  post: string | null;
}

const LISTS = new Set(['like', 'dislike', 'favouriteTag', 'blacklist', 'savedSearch', 'followedArtist']);
const USERNAME = /^[a-z0-9_.-]{3,32}$/;
// the app stretches the password with argon2 and sends the 32 byte result as hex
const DERIVED_KEY = /^[0-9a-f]{64}$/;
const MAX_KEY_LENGTH = 512;
const MAX_POST_LENGTH = 16_384;
// a push is one version, and keeping it below PULL_LIMIT means a pull page always holds whole versions
const PUSH_LIMIT = 200;
const PULL_LIMIT = 500;

class HttpError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    try {
      if (request.method !== 'POST') throw new HttpError(405, 'Only POST is supported');
      const body: unknown = await request.json().catch(() => {
        throw new HttpError(400, 'The request body must be JSON');
      });
      switch (new URL(request.url).pathname) {
        case '/signup':
          return json(await signUp(env, body));
        case '/login':
          return json(await logIn(env, body));
        case '/logout':
          await logOut(env, request);
          return json({});
        case '/push':
          await push(env, await userFor(env, request), body);
          return json({});
        case '/pull':
          return json(await pull(env, await userFor(env, request), body));
        default:
          throw new HttpError(404, 'Unknown endpoint');
      }
    } catch (error) {
      if (error instanceof HttpError) return json({ error: error.message }, error.status);
      console.error(error);
      return json({ error: 'The sync server had a problem' }, 500);
    }
  },
} satisfies ExportedHandler<Env>;

function json(value: unknown, status = 200) {
  return Response.json(value, { status });
}

async function sha256(text: string) {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function credentials(body: unknown) {
  const { username, key } = (body ?? {}) as Record<string, unknown>;
  if (typeof username !== 'string' || !USERNAME.test(username)) {
    throw new HttpError(400, 'Usernames are 3 to 32 lowercase letters, numbers, dots, dashes or underscores');
  }
  if (typeof key !== 'string' || !DERIVED_KEY.test(key)) throw new HttpError(400, 'Malformed login');
  return { username, key };
}

async function startSession(env: Env, userId: number) {
  const token = [...crypto.getRandomValues(new Uint8Array(32))].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  await env.DB.prepare('INSERT INTO sessions (token_hash, user_id, created_at) VALUES (?1, ?2, ?3)')
    .bind(await sha256(token), userId, Date.now())
    .run();
  return { token };
}

async function signUp(env: Env, body: unknown) {
  const { username, key } = credentials(body);
  const created = await env.DB.prepare(
    'INSERT INTO users (username, key_hash, created_at) VALUES (?1, ?2, ?3) ON CONFLICT (username) DO NOTHING RETURNING id',
  )
    .bind(username, await sha256(key), Date.now())
    .first<{ id: number }>();
  if (!created) throw new HttpError(409, 'That username is taken');
  return startSession(env, created.id);
}

async function logIn(env: Env, body: unknown) {
  const { username, key } = credentials(body);
  const user = await env.DB.prepare('SELECT id, key_hash FROM users WHERE username = ?1')
    .bind(username)
    .first<{ id: number; key_hash: string }>();
  const given = new TextEncoder().encode(await sha256(key));
  const stored = new TextEncoder().encode(user?.key_hash ?? '0'.repeat(64));
  if (!crypto.subtle.timingSafeEqual(given, stored) || !user) throw new HttpError(401, 'Wrong username or password');
  return startSession(env, user.id);
}

function bearerToken(request: Request) {
  const header = request.headers.get('Authorization') ?? '';
  return header.startsWith('Bearer ') ? header.slice('Bearer '.length) : null;
}

async function userFor(env: Env, request: Request) {
  const token = bearerToken(request);
  const session = token
    ? await env.DB.prepare('SELECT user_id FROM sessions WHERE token_hash = ?1')
        .bind(await sha256(token))
        .first<{ user_id: number }>()
    : null;
  if (!session) throw new HttpError(401, 'Logged out, log in again');
  return session.user_id;
}

async function logOut(env: Env, request: Request) {
  const token = bearerToken(request);
  if (token) await env.DB.prepare('DELETE FROM sessions WHERE token_hash = ?1').bind(await sha256(token)).run();
}

// apps from before the merged database sync each site apart, keyed by site post ids, which would mix with md5 keys
function refuseOldApps(body: unknown) {
  if (typeof body === 'object' && body !== null && 'site' in body) throw new HttpError(400, 'Update the app to sync');
}

function validItem(value: unknown): value is Item {
  const item = value as Item;
  return (
    typeof item === 'object' &&
    item !== null &&
    LISTS.has(item.list) &&
    typeof item.key === 'string' &&
    item.key.length > 0 &&
    item.key.length <= MAX_KEY_LENGTH &&
    typeof item.present === 'boolean' &&
    Number.isSafeInteger(item.sortAt) &&
    Number.isSafeInteger(item.changedAt) &&
    (item.post === null || (typeof item.post === 'object' && JSON.stringify(item.post).length <= MAX_POST_LENGTH))
  );
}

async function push(env: Env, userId: number, body: unknown) {
  refuseOldApps(body);
  const { items } = body as { items: unknown };
  if (!Array.isArray(items) || items.length > PUSH_LIMIT || !items.every(validItem)) {
    throw new HttpError(400, 'Malformed changes');
  }
  if (items.length === 0) return;
  // the free plan allows 50 queries per request, so every item goes in through one json_each statement.
  // the newest change wins when two devices changed the same item
  await env.DB.batch([
    env.DB.prepare('UPDATE users SET version = version + 1 WHERE id = ?1').bind(userId),
    env.DB.prepare(
      `INSERT INTO items (user_id, list, key, present, sort_at, changed_at, version, post)
       SELECT ?1, value ->> 'list', value ->> 'key', value ->> 'present', value ->> 'sortAt', value ->> 'changedAt',
              (SELECT version FROM users WHERE id = ?1), value ->> 'post'
       FROM json_each(?2) WHERE true
       ON CONFLICT (user_id, list, key) DO UPDATE SET
         present = excluded.present, sort_at = excluded.sort_at, changed_at = excluded.changed_at,
         version = excluded.version, post = excluded.post
       WHERE excluded.changed_at >= items.changed_at`,
    ).bind(userId, JSON.stringify(items)),
  ]);
}

async function pull(env: Env, userId: number, body: unknown) {
  refuseOldApps(body);
  const { since } = body as { since: unknown };
  if (!Number.isSafeInteger(since)) throw new HttpError(400, 'Malformed cursor');
  const { results } = await env.DB.prepare(
    `SELECT list, key, present, sort_at, changed_at, version, post FROM items
     WHERE user_id = ?1 AND version > ?2 ORDER BY version LIMIT ?3`,
  )
    .bind(userId, since, PULL_LIMIT + 1)
    .all<ItemRow>();
  const more = results.length > PULL_LIMIT;
  // drop the last version when it might be cut off, the next pull starts from it again
  const rows = more ? results.filter((row) => row.version !== results[results.length - 1].version) : results;
  return {
    items: rows.map((row) => ({
      list: row.list,
      key: row.key,
      present: row.present === 1,
      sortAt: row.sort_at,
      changedAt: row.changed_at,
      post: row.post === null ? null : JSON.parse(row.post),
    })),
    cursor: rows.length ? rows[rows.length - 1].version : since,
    more,
  };
}
