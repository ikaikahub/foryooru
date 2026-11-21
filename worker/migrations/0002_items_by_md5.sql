-- the app keeps one database for both sites now, so items no longer belong to a site. likes and dislikes are keyed
-- by the file's md5 (or site:id for a post without one), and their post holds each site's view of it
CREATE TABLE items_next (
  user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  list       TEXT NOT NULL,
  key        TEXT NOT NULL,
  present    INTEGER NOT NULL CHECK (present IN (0, 1)),
  sort_at    INTEGER NOT NULL,
  changed_at INTEGER NOT NULL,
  version    INTEGER NOT NULL,
  post       TEXT,
  PRIMARY KEY (user_id, list, key)
);

-- tag lists, saved searches and followed artists keep their keys. where both sites had one, the newest change wins
INSERT INTO items_next (user_id, list, key, present, sort_at, changed_at, version, post)
SELECT user_id, list, key, present, sort_at, changed_at, version, NULL FROM (
  SELECT *, ROW_NUMBER() OVER (PARTITION BY user_id, list, key ORDER BY changed_at DESC, version DESC) AS newest
  FROM items WHERE list NOT IN ('like', 'dislike')
) WHERE newest = 1;

-- likes and dislikes take their key from the post they carry. removals carry no post, so they can't be moved and
-- are dropped: every device syncs before the update, so none are waiting
INSERT INTO items_next (user_id, list, key, present, sort_at, changed_at, version, post)
SELECT user_id, list, new_key, present, sort_at, changed_at, version, new_post FROM (
  SELECT user_id, list, present, sort_at, changed_at, version, new_key,
    json_object(
      'md5', NULLIF(lower(post ->> 'md5'), ''),
      'width', post ->> 'width',
      'height', post ->> 'height',
      'sites', json_object(site, json_remove(post, '$.md5', '$.width', '$.height'))
    ) AS new_post,
    ROW_NUMBER() OVER (PARTITION BY user_id, list, new_key ORDER BY changed_at DESC, version DESC) AS newest
  FROM (
    SELECT *, CASE WHEN COALESCE(post ->> 'md5', '') = '' THEN site || ':' || (post ->> 'id')
                   ELSE lower(post ->> 'md5') END AS new_key
    FROM items WHERE list IN ('like', 'dislike') AND post IS NOT NULL
  )
) WHERE newest = 1;

DROP TABLE items;
ALTER TABLE items_next RENAME TO items;
CREATE INDEX items_by_version ON items (user_id, version);
