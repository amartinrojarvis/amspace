#!/usr/bin/env python3
"""Recompress embedded ICNS PNG streams without changing pixels or metadata."""
import struct
import sys
import zlib
from pathlib import Path


def png_chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))


def compress_png(png):
    if not png.startswith(b"\x89PNG\r\n\x1a\n"):
        return png
    chunks = []
    offset = 8
    while offset < len(png):
        size, tag = struct.unpack_from(">I4s", png, offset)
        end = offset + size + 12
        if end > len(png):
            raise ValueError("Truncated PNG chunk")
        chunks.append((tag, png[offset + 8:offset + size + 8], png[offset:end]))
        offset = end
    source = b"".join(data for tag, data, _ in chunks if tag == b"IDAT")
    pixels = zlib.decompress(source)
    compressed = zlib.compress(pixels, 9)
    if len(compressed) >= len(source):
        return png
    assert zlib.decompress(compressed) == pixels
    result = [png[:8]]
    written = False
    for tag, _, original in chunks:
        if tag != b"IDAT":
            result.append(original)
        elif not written:
            result.append(png_chunk(tag, compressed))
            written = True
    return b"".join(result)


def optimize(path):
    source = path.read_bytes()
    if source[:4] != b"icns" or struct.unpack_from(">I", source, 4)[0] != len(source):
        raise ValueError("Invalid ICNS header")
    entries = []
    offset = 8
    while offset < len(source):
        tag, size = struct.unpack_from(">4sI", source, offset)
        if size < 8 or offset + size > len(source):
            raise ValueError("Invalid ICNS entry")
        data = compress_png(source[offset + 8:offset + size])
        entries.append(struct.pack(">4sI", tag, len(data) + 8) + data)
        offset += size
    body = b"".join(entries)
    result = b"icns" + struct.pack(">I", len(body) + 8) + body
    if len(result) < len(source):
        path.write_bytes(result)
    print(f"{path}: {len(source)} → {min(len(result), len(source))} bytes")


if __name__ == "__main__":
    optimize(Path(sys.argv[1] if len(sys.argv) > 1 else "assets/AMSpace.icns"))
