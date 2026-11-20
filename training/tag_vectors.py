import subprocess
import sys
import time
from pathlib import Path

import numpy as np
from scipy import sparse
from scipy.sparse.linalg import svds

VOCABULARY = 32_000
DIMENSIONS = 64
CHUNK_POSTS = 500_000
CONTEXT_SMOOTHING = 0.75
SVD_OVERSAMPLING = 16
SVD_ITERATIONS = 6
SKIPPED_TYPES = (5, 6)
OUT = Path(__file__).parent / "out"

PSQL = ["docker", "exec", "kimochii-db", "psql", "-U", "nice", "-d", "kimochii", "-At"]


def query(sql):
  result = subprocess.run([*PSQL, "-F", "\t", "-c", sql], capture_output=True, text=True, encoding="utf-8", check=True)
  return [line.split("\t") for line in result.stdout.splitlines() if line]


def vocabulary():
  rows = query(f"""
    SELECT t.id, t.key, coalesce(max(g.type), 0), t.count
    FROM tags t LEFT JOIN gelbooru_tags g ON g.linked_to = t.id
    GROUP BY t.id
    HAVING coalesce(max(g.type), 0) NOT IN {SKIPPED_TYPES}
    ORDER BY t.count DESC
    LIMIT {VOCABULARY}
  """)
  ids = np.array([int(row[0]) for row in rows])
  names = [row[1] for row in rows]
  kinds = np.array([int(row[2]) for row in rows], dtype=np.int8)
  counts = np.array([int(row[3]) for row in rows])
  return ids, names, kinds, counts


def column_lookup(ids):
  lookup = np.full(ids.max() + 1, -1, dtype=np.int32)
  lookup[ids] = np.arange(len(ids), dtype=np.int32)
  return lookup


def add_chunk(together, lookup, scores, lengths, tag_ids):
  columns = np.full(len(tag_ids), -1, dtype=np.int32)
  known = tag_ids < len(lookup)
  columns[known] = lookup[tag_ids[known]]
  rows = np.repeat(np.arange(len(lengths)), lengths)
  weights = np.repeat(np.sqrt(np.log(2.0 + np.maximum(scores, 0))), lengths)
  kept = columns >= 0
  posts = sparse.csr_matrix(
    (weights[kept].astype(np.float32), (rows[kept], columns[kept])),
    shape=(len(lengths), together.shape[0]),
  )
  pairs = (posts.T @ posts).tocoo()
  together[pairs.row, pairs.col] += pairs.data


def count_pairs(lookup, size):
  together = np.zeros((size, size), dtype=np.float32)
  command = [*PSQL, "-c", "COPY (SELECT p.score, pt.tags FROM post_tags pt JOIN posts p ON p.id = pt.post) TO STDOUT"]
  process = subprocess.Popen(command, stdout=subprocess.PIPE, text=True, encoding="utf-8", bufsize=1 << 24)
  scores, lengths, texts, seen, started = [], [], [], 0, time.time()

  def flush():
    joined = ",".join(text for text in texts if text)
    tag_ids = np.array(joined.split(","), dtype=np.int64) if joined else np.zeros(0, dtype=np.int64)
    add_chunk(together, lookup, np.array(scores, dtype=np.float64), np.array(lengths), tag_ids)
    scores.clear()
    lengths.clear()
    texts.clear()

  for line in process.stdout:
    score, array = line.rstrip("\n").split("\t")
    inner = array[1:-1]
    scores.append(int(score))
    texts.append(inner)
    lengths.append(inner.count(",") + 1 if inner else 0)
    seen += 1
    if len(scores) == CHUNK_POSTS:
      flush()
      print(f"{seen:,} posts counted in {time.time() - started:.0f}s", flush=True)
  if scores:
    flush()
  if process.wait() != 0:
    sys.exit(f"psql failed with exit code {process.returncode}")
  print(f"{seen:,} posts counted in {time.time() - started:.0f}s", flush=True)
  return together


def saved_pairs(ids):
  counts, tags = OUT / "pair_counts.npy", OUT / "pair_tags.npy"
  if not counts.exists() or not tags.exists() or not np.array_equal(np.load(tags), ids):
    return None
  print("reusing saved pair counts", flush=True)
  return np.load(counts)


def positive_pmi(together):
  np.fill_diagonal(together, 0)
  totals = together.sum(axis=1, dtype=np.float64)
  smoothed = totals ** CONTEXT_SMOOTHING
  context = smoothed / smoothed.sum()
  blocks = []
  for start in range(0, len(together), 2000):
    block = together[start:start + 2000].astype(np.float64)
    with np.errstate(divide="ignore", invalid="ignore"):
      pmi = np.log(block / (totals[start:start + 2000, None] * context[None, :]))
    pmi[~np.isfinite(pmi) | (pmi < 0)] = 0
    blocks.append(sparse.csr_matrix(pmi.astype(np.float32)))
  return sparse.vstack(blocks).tocsr()


def unit_rows(vectors):
  norms = np.linalg.norm(vectors, axis=1, keepdims=True)
  return (vectors / np.where(norms == 0, 1, norms)).astype(np.float32)


def embed_on_cpu(together):
  ppmi = positive_pmi(together)
  print(f"positive PMI: {ppmi.nnz:,} pairs", flush=True)
  left, strengths, _ = svds(ppmi, k=DIMENSIONS)
  return unit_rows(left * np.sqrt(strengths)[None, :])


def embed_on_gpu(torch, together):
  matrix = torch.from_numpy(together).cuda()
  matrix.fill_diagonal_(0)
  totals = matrix.sum(dim=1, dtype=torch.float64)
  smoothed = totals ** CONTEXT_SMOOTHING
  context = (smoothed / smoothed.sum()).float()
  matrix.div_(totals.float()[:, None]).div_(context[None, :]).log_().clamp_(min=0).nan_to_num_(0)
  print(f"positive PMI: {int((matrix > 0).sum()):,} pairs (GPU)", flush=True)
  left, strengths, _ = torch.svd_lowrank(matrix, q=DIMENSIONS + SVD_OVERSAMPLING, niter=SVD_ITERATIONS)
  vectors = left[:, :DIMENSIONS] * strengths[:DIMENSIONS].sqrt()[None, :]
  return unit_rows(vectors.cpu().numpy())


def embed(together):
  try:
    import torch
  except ImportError:
    torch = None
  if torch is not None and torch.cuda.is_available():
    return embed_on_gpu(torch, together)
  return embed_on_cpu(together)


def nearest(vectors, names, name, count=8):
  if name not in names:
    return f"{name}: not in vocabulary"
  index = names.index(name)
  closeness = vectors @ vectors[index]
  order = [other for other in np.argsort(-closeness) if other != index][:count]
  return f"{name}: " + ", ".join(f"{names[other]} {closeness[other]:.2f}" for other in order)


def main():
  OUT.mkdir(exist_ok=True)
  ids, names, kinds, counts = vocabulary()
  print(f"vocabulary: {len(ids):,} tags, smallest has {counts[-1]:,} posts", flush=True)
  together = saved_pairs(ids)
  if together is None:
    together = count_pairs(column_lookup(ids), len(ids))
    np.save(OUT / "pair_counts.npy", together)
    np.save(OUT / "pair_tags.npy", ids)
  started = time.time()
  vectors = embed(together)
  print(f"vectors built in {time.time() - started:.0f}s", flush=True)
  np.savez_compressed(OUT / "tag_vectors.npz", names=np.array(names), kinds=kinds, counts=counts, vectors=vectors)
  print(f"saved {OUT / 'tag_vectors.npz'}", flush=True)
  for name in ("cat_ears", "tentacles", "hatsune_miku", "thighhighs", "shibari", "knitting", "fox_girl", "lalafell"):
    print(nearest(vectors, names, name), flush=True)


if __name__ == "__main__":
  main()
