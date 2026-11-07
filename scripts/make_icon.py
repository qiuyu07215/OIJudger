"""Render the OIJudger icon (rounded terracotta square with a check mark) to a PNG."""
import math
import struct
import sys
import zlib

N = 1024


def inside_rounded(x, y, r=0.22):
    m = 0.06
    if not (m <= x <= 1 - m and m <= y <= 1 - m):
        return 0.0
    cx = min(max(x, m + r), 1 - m - r)
    cy = min(max(y, m + r), 1 - m - r)
    d = math.hypot(x - cx, y - cy)
    return max(0.0, min(1.0, (r - d) * N + 0.5))


def seg_dist(px, py, ax, ay, bx, by):
    vx, vy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy)))
    return math.hypot(px - ax - t * vx, py - ay - t * vy)


def main(out):
    pts = [(0.28, 0.52), (0.43, 0.67), (0.73, 0.35)]
    w = 0.075
    rows = []
    for j in range(N):
        row = bytearray([0])
        y = (j + 0.5) / N
        for i in range(N):
            x = (i + 0.5) / N
            a = inside_rounded(x, y)
            if a <= 0:
                row += b"\0\0\0\0"
                continue
            # warm terracotta (#D97757) with a soft vertical gradient
            g = 1.04 - 0.12 * y
            col = [min(255, int(217 * g)), int(119 * g), int(87 * g)]
            d = min(seg_dist(x, y, *pts[0], *pts[1]), seg_dist(x, y, *pts[1], *pts[2]))
            k = max(0.0, min(1.0, (w - d) * N + 0.5))
            col = [int(c * (1 - k) + w_ * k) for c, w_ in zip(col, (250, 249, 245))]
            row += bytes(col + [int(255 * a)])
        rows.append(bytes(row))
    raw = zlib.compress(b"".join(rows), 9)

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", N, N, 8, 6, 0, 0, 0)) + chunk(b"IDAT", raw) + chunk(b"IEND", b"")
    with open(out, "wb") as f:
        f.write(png)


if __name__ == "__main__":
    main(sys.argv[1])
