#!/usr/bin/env python3
"""Download Danbooru users' favourites into a SQLite file, including random users.

Uses GET /posts.json?tags=ordfav:<username>, which returns full post records in
favourite order, so no per-post requests are needed.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import sqlite3
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import requests

API_URL = "https://danbooru.donmai.us"
PAGE_SIZE = 200  # the maximum limit for /posts.json
FAVOURITES_LIMIT = 1000  # the maximum limit for /favorites.json
FAVOURITES_MAX_PAGES = 1000  # anonymous and member pagination limit
USER_LOOKUP_SIZE = 100  # user ids per /users.json lookup
DEFAULT_MAX_PAGES = 1000  # anonymous and member limit; Gold+ get 5000
REQUEST_PAUSE = 1.0  # seconds between request starts, per Danbooru's advice for longer sessions
MAX_RETRIES = 5
RETRY_STATUSES = {429, 500, 502, 503, 504}

_last_request_at = 0.0


def pace() -> None:
  """Keep at least REQUEST_PAUSE seconds between the starts of any two requests."""
  global _last_request_at
  wait = REQUEST_PAUSE - (time.monotonic() - _last_request_at)
  if wait > 0:
    time.sleep(wait)
  _last_request_at = time.monotonic()


def request_json(
  session: requests.Session, path: str, params: dict, description: str
) -> list[dict] | None:
  """GET a Danbooru endpoint, retrying transient failures; None means 410 Gone."""
  for attempt in range(1, MAX_RETRIES + 1):
    wait = 2.0 * attempt
    pace()
    try:
      response = session.get(f"{API_URL}/{path}", params=params, timeout=60)
    except requests.RequestException as error:
      failure = str(error)
    else:
      if response.status_code == 200:
        return response.json()
      if response.status_code == 410:
        return None
      failure = f"HTTP {response.status_code}: {response.text.strip()[:200]}"
      if response.status_code not in RETRY_STATUSES:
        raise RuntimeError(f"Danbooru rejected the request for {description}: {failure}")
      retry_after = response.headers.get("Retry-After", "")
      if retry_after.isdigit():
        wait = max(wait, int(retry_after))

    if attempt < MAX_RETRIES:
      print(f"  retry {attempt}/{MAX_RETRIES - 1} in {wait:.0f}s ({failure})", file=sys.stderr)
      time.sleep(wait)

  raise RuntimeError(f"Danbooru request failed for {description}: {failure}")


def scrape_user(session: requests.Session, username: str, max_pages: int) -> list[dict]:
  """Collect every favourite post for one user, newest favourite first."""
  posts: list[dict] = []
  seen: set[int] = set()

  for page in range(1, max_pages + 1):
    batch = request_json(
      session,
      "posts.json",
      {"tags": f"ordfav:{username}", "limit": PAGE_SIZE, "page": page},
      f"{username} (page {page})",
    )
    if batch is None:
      print(f"{username}: stopped at Danbooru's page limit", file=sys.stderr)
      break

    new = [post for post in batch if post["id"] not in seen]
    seen.update(post["id"] for post in new)
    posts.extend(new)
    print(f"{username}: page {page}, {len(posts)} posts", file=sys.stderr)

    if len(batch) < PAGE_SIZE:
      break
  else:
    print(f"{username}: stopped at --max-pages {max_pages}", file=sys.stderr)

  return posts


def sample_usernames(session: requests.Session, count: int) -> list[str]:
  """Pick random users who have public favourites, from random favourites pages."""
  pages = list(range(1, FAVOURITES_MAX_PAGES + 1))
  random.shuffle(pages)

  candidate_ids: list[int] = []
  seen_ids: set[int] = set()
  for page in pages:
    rows = request_json(
      session,
      "favorites.json",
      {"limit": FAVOURITES_LIMIT, "page": page},
      f"favourites page {page}",
    )
    for row in rows or []:
      if row["user_id"] not in seen_ids:
        seen_ids.add(row["user_id"])
        candidate_ids.append(row["user_id"])
    if len(candidate_ids) >= count:
      break
  else:
    raise RuntimeError(f"Only found {len(candidate_ids)} users with public favourites")

  random.shuffle(candidate_ids)
  usernames: list[str] = []
  for start in range(0, len(candidate_ids), USER_LOOKUP_SIZE):
    chunk = candidate_ids[start : start + USER_LOOKUP_SIZE]
    users = request_json(
      session,
      "users.json",
      {"search[id]": ",".join(str(user_id) for user_id in chunk), "limit": len(chunk)},
      "user lookup",
    )
    if users:
      usernames.extend(user["name"] for user in users)
    if len(usernames) >= count:
      break

  if len(usernames) < count:
    print(f"Only found {len(usernames)} of {count} users", file=sys.stderr)
  return usernames[:count]


# posts are stored once however many users favourite them; tag columns hold Danbooru's space-separated tag strings
SCHEMA = """
CREATE TABLE IF NOT EXISTS users (
 id INTEGER PRIMARY KEY,
 name TEXT NOT NULL UNIQUE,
 fetched_at TEXT NOT NULL,
 favourites INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS posts (
 id INTEGER PRIMARY KEY,
 md5 TEXT,
 rating TEXT,
 score INTEGER,
 fav_count INTEGER,
 created_at TEXT,
 artists TEXT NOT NULL,
 characters TEXT NOT NULL,
 copyrights TEXT NOT NULL,
 general TEXT NOT NULL,
 -- the whole Danbooru post record, for anything the columns leave out
 data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS favourites (
 user_id INTEGER NOT NULL REFERENCES users (id),
 -- 0 is the newest favourite
 position INTEGER NOT NULL,
 post_id INTEGER NOT NULL REFERENCES posts (id),
 PRIMARY KEY (user_id, position)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS favourites_by_post ON favourites (post_id);
"""


def open_db(path: Path) -> sqlite3.Connection:
  path.parent.mkdir(parents=True, exist_ok=True)
  db = sqlite3.connect(path)
  db.executescript(SCHEMA)
  return db


def is_fetched(db: sqlite3.Connection, username: str) -> bool:
  return db.execute("SELECT 1 FROM users WHERE name = ?", (username,)).fetchone() is not None


def save_user(db: sqlite3.Connection, username: str, posts: list[dict]) -> None:
  """Store one user's favourites in a single transaction, so an interrupted run never leaves half a user."""
  with db:
    db.executemany(
      """INSERT INTO posts (id, md5, rating, score, fav_count, created_at, artists, characters, copyrights, general, data)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT (id) DO UPDATE SET score = excluded.score, fav_count = excluded.fav_count, data = excluded.data""",
      [
        (
          post["id"],
          post.get("md5"),
          post.get("rating"),
          post.get("score"),
          post.get("fav_count"),
          post.get("created_at"),
          post.get("tag_string_artist", ""),
          post.get("tag_string_character", ""),
          post.get("tag_string_copyright", ""),
          post.get("tag_string_general", ""),
          json.dumps(post, ensure_ascii=False, separators=(",", ":")),
        )
        for post in posts
      ],
    )
    user_id = db.execute(
      """INSERT INTO users (name, fetched_at, favourites) VALUES (?, ?, ?)
       ON CONFLICT (name) DO UPDATE SET fetched_at = excluded.fetched_at, favourites = excluded.favourites
       RETURNING id""",
      (username, datetime.now(timezone.utc).isoformat(timespec="seconds"), len(posts)),
    ).fetchone()[0]
    db.execute("DELETE FROM favourites WHERE user_id = ?", (user_id,))
    db.executemany(
      "INSERT INTO favourites (user_id, position, post_id) VALUES (?, ?, ?)",
      [(user_id, position, post["id"]) for position, post in enumerate(posts)],
    )


def user_agent() -> str:
  """Danbooru asks bots to identify themselves with a unique User-Agent."""
  return os.environ.get("DANBOORU_USER_AGENT", "").strip() or "foryooru-favourites/1.0"


def credentials() -> tuple[str, str] | None:
  login = os.environ.get("DANBOORU_LOGIN", "").strip()
  api_key = os.environ.get("DANBOORU_API_KEY", "").strip()
  return (login, api_key) if login and api_key else None


def main(argv: list[str] | None = None) -> int:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("users", nargs="*", help="Danbooru usernames to download favourites for")
  parser.add_argument(
    "--db", type=Path, default=Path("favourites.sqlite"), help="SQLite file to add to (default: %(default)s)"
  )
  parser.add_argument(
    "--max-pages", type=int, default=DEFAULT_MAX_PAGES, help="stop after this many pages (default: %(default)s)"
  )
  parser.add_argument(
    "--random-users",
    type=int,
    metavar="N",
    help="also download favourites for N random users with public favourites",
  )
  args = parser.parse_args(argv)
  if not args.users and args.random_users is None:
    parser.error("give at least one username or --random-users N")
  if args.max_pages < 1:
    parser.error("--max-pages must be at least 1")
  if args.random_users is not None and args.random_users < 1:
    parser.error("--random-users must be at least 1")

  session = requests.Session()
  session.headers["User-Agent"] = user_agent()
  auth = credentials()
  if auth:
    session.auth = auth
  else:
    print(
      "No DANBOORU_LOGIN / DANBOORU_API_KEY set; only public favourites will work.",
      file=sys.stderr,
    )

  usernames = list(args.users)
  if args.random_users is not None:
    sampled = sample_usernames(session, args.random_users)
    print(f"Random users: {', '.join(sampled)}", file=sys.stderr)
    usernames.extend(sampled)
  usernames = list(dict.fromkeys(usernames))

  db = open_db(args.db)
  started = time.monotonic()
  fetched = 0
  for index, username in enumerate(usernames, 1):
    progress = f"[{index}/{len(usernames)}]"
    # lets a long run pick up where it stopped
    if is_fetched(db, username):
      print(f"{progress} {username}: already in {args.db}, skipped")
      continue
    posts = scrape_user(session, username, args.max_pages)
    save_user(db, username, posts)
    fetched += 1
    # skipped users take no time, so the estimate only averages the fetched ones
    elapsed = time.monotonic() - started
    remaining = elapsed / fetched * (len(usernames) - index)
    timing = f"{elapsed / 60:.0f} min so far, about {remaining / 60:.0f} min left"
    if posts:
      print(f"{progress} {username}: {len(posts)} posts saved, {timing}")
    else:
      print(f"{progress} {username}: no favourites found (unknown user, or an empty list), {timing}")
  db.close()

  return 0


if __name__ == "__main__":
  try:
    sys.exit(main())
  except RuntimeError as error:
    print(error, file=sys.stderr)
    sys.exit(1)
