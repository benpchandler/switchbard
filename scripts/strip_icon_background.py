#!/usr/bin/env python3
"""
Replace the white border around assets/icon.png with transparency so the dock
doesn't render a white square behind the squircle.

Flood-fills from the four corners across near-white pixels and zeroes their
alpha. Internal whites (highlight dots inside the artwork) are unreachable
from the corners and stay intact.
"""

from __future__ import annotations

import sys
from collections import deque
from pathlib import Path

from PIL import Image

# Pixels within this Euclidean RGB distance of pure white count as "background"
# for flood-fill purposes. Tuned for the existing icon, where the background is
# exactly (255, 255, 255) and the squircle edge transitions sharply to dark.
WHITE_TOLERANCE = 24


def is_white(px: tuple[int, int, int], tol: int = WHITE_TOLERANCE) -> bool:
    r, g, b = px[:3]
    return (255 - r) ** 2 + (255 - g) ** 2 + (255 - b) ** 2 <= tol * tol


def alpha_from_corner_flood(img: Image.Image) -> Image.Image:
    rgba = img.convert("RGBA")
    w, h = rgba.size
    pixels = rgba.load()
    alpha_mask = bytearray(w * h)  # 0 = transparent, 255 = opaque
    for i in range(w * h):
        alpha_mask[i] = 255

    visited = bytearray(w * h)
    queue: deque[tuple[int, int]] = deque()
    for corner in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]:
        if is_white(pixels[corner]):
            queue.append(corner)
            visited[corner[1] * w + corner[0]] = 1
            alpha_mask[corner[1] * w + corner[0]] = 0

    while queue:
        x, y = queue.popleft()
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if not (0 <= nx < w and 0 <= ny < h):
                continue
            idx = ny * w + nx
            if visited[idx]:
                continue
            if not is_white(pixels[nx, ny]):
                continue
            visited[idx] = 1
            alpha_mask[idx] = 0
            queue.append((nx, ny))

    rgba.putalpha(Image.frombytes("L", (w, h), bytes(alpha_mask)))
    return rgba


def main() -> int:
    src = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(
        "crates/hive-gui/assets/icon.png"
    )
    if not src.exists():
        print(f"missing: {src}", file=sys.stderr)
        return 1
    out = alpha_from_corner_flood(Image.open(src))
    out.save(src, optimize=True)
    print(f"✓ wrote {src} ({out.mode}, {out.size[0]}x{out.size[1]})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
