#!/usr/bin/env python3
"""Write a 256x256 PNG icon without third-party libraries."""

import struct
import zlib
from pathlib import Path


def chunk(tag: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


def png(size: int = 256) -> bytes:
    rows = []
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            edge = min(x, y, size - 1 - x, size - 1 - y)
            if edge < 18:
                row.extend((26, 20, 15))
            elif (x // 32 + y // 32) % 2 == 0:
                row.extend((47, 93, 69))
            else:
                row.extend((234, 215, 176))
            if 108 <= x <= 148 and 70 <= y <= 170:
                row[-3:] = bytes((226, 177, 90))
        rows.append(bytes(row))
    raw = b"".join(rows)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def main() -> None:
    target = Path(__file__).resolve().parents[1] / "ui" / "public" / "icon.png"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(png())
    print(target)


if __name__ == "__main__":
    main()
