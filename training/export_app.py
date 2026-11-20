import struct
from pathlib import Path

import numpy as np

MAGIC = b"FYTV"
SOURCE = Path(__file__).parent / "out" / "tag_vectors.npz"
TARGET = Path(__file__).parent.parent / "src-tauri" / "assets" / "tag_vectors.bin"


def main():
  saved = np.load(SOURCE)
  names = [str(name) for name in saved["names"]]
  vectors = saved["vectors"]
  quantized = np.clip(np.round(vectors * 127), -127, 127).astype(np.int8)
  restored = quantized.astype(np.float32)
  restored /= np.linalg.norm(restored, axis=1, keepdims=True)
  sample = np.random.default_rng(0).choice(len(names), 2000, replace=False)
  drift = np.abs((vectors[sample] @ vectors[sample].T) - (restored[sample] @ restored[sample].T)).max()
  print(f"largest similarity change from quantizing: {drift:.4f}")

  TARGET.parent.mkdir(parents=True, exist_ok=True)
  with TARGET.open("wb") as file:
    file.write(MAGIC)
    file.write(struct.pack("<II", len(names), vectors.shape[1]))
    for name in names:
      encoded = name.encode("utf-8")
      file.write(struct.pack("<H", len(encoded)))
      file.write(encoded)
    file.write(quantized.tobytes())
  print(f"wrote {TARGET} ({TARGET.stat().st_size / 1_000_000:.1f} MB)")


if __name__ == "__main__":
  main()
