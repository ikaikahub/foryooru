# Danbooru favourites scraper

Saves users' favourites into a SQLite file. Full post records come straight from
`GET /posts.json?tags=ordfav:<username>`, so there are no per-post requests.

## Usage

```bash
python get_favourites.py USERNAME [USERNAME...]
python get_favourites.py alice bob --db data.sqlite
```

For a sample of random users who have public favourites:

```bash
python get_favourites.py --random-users 50
python get_favourites.py --random-users 50 alice --db data.sqlite --max-pages 25
```

The sample comes from random pages of `/favorites.json` (1000 rows each), whose
rows carry `user_id`. Those ids are resolved to usernames through `/users.json`
and printed before downloading. This avoids `/users.json` sampling, where most
accounts have no favourites at all.

Everything goes into `favourites.sqlite` (or `--db`), in three tables:

- `users`: `name`, `fetched_at`, `favourites` (how many were saved)
- `posts`: each post once however many users favourite it. `md5`, `rating`,
  `score`, `fav_count`, `created_at` and the tag strings (`artists`,
  `characters`, `copyrights`, `general`) are columns, and `data` holds the whole
  Danbooru post record as JSON for anything else (`json_extract(data, '$.file_ext')`)
- `favourites`: `user_id`, `post_id` and `position`, where 0 is the newest favourite

Each user is saved in one transaction, so an interrupted run never leaves half a
user. Users already in the file are skipped, so rerunning picks up where it
stopped. Delete a user's rows to fetch them again.

The replay reads this file directly:

```bash
src-tauri/target/release/foryooru-server replay-users pool/kimochii.snapshot danbooru-favourites/favourites.sqlite pool/replay-users.md
```

## Credentials

Optional. Public favourites work anonymously; private favourites and higher
limits need an API key from https://danbooru.donmai.us/profile:

```bash
DANBOORU_LOGIN=your_name DANBOORU_API_KEY=your_key python get_favourites.py USERNAME
```

Private favourites (Gold+) never appear in anonymous searches or in the
`/favorites.json` sampling, so they're only reachable for your own account.

Set `DANBOORU_USER_AGENT` to identify your bot, e.g.
`"my-scraper/1.0 (user #12345)"` (Danbooru asks for a user ID in the UA).

## Limits

- `/posts.json` returns at most 200 posts per page.
- Anonymous and member accounts can read up to 1000 pages (200,000 posts);
  Gold and above get 5000. `--max-pages` caps it lower per user.
- Every request (pages, user lookups, retries) starts at least 1 second after
  the previous one, near Danbooru's recommended 1 request per second.
  429/5xx responses are retried with backoff, honouring `Retry-After`.
- A `410 Gone` means the page limit was reached; the script keeps what it got.
- `/favorites.json` is also capped at 1000 pages, so sampling spans the site's
  favourite history but not more than that.

Favourite timestamps are not included: they live in `/favorites.json`, not in
the post records. Add a second `/favorites.json?search[user_id]=...` pass and
join on `post_id` if you need them.
