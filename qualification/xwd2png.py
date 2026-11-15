#!/usr/bin/env python3
"""Convert an `xwd -root` screen dump (24/32-bit TrueColor) to PNG and report
how many distinct colours it has, so a blank window can be detected.

Usage: xwd2png.py IN.xwd OUT.png [--min-colors N]
Exits 1 if the image has fewer than N distinct colours (default 0: no check).
Python 3.6+ standard library only (runs on old distributions).
"""
import struct
import sys
import zlib


def main():
    args = sys.argv[1:]
    min_colors = 0
    if "--min-colors" in args:
        i = args.index("--min-colors")
        min_colors = int(args[i + 1])
        del args[i:i + 2]
    src, dst = args
    data = open(src, "rb").read()
    h = struct.unpack(">25I", data[:100])
    header_size, depth, width, height = h[0], h[3], h[4], h[5]
    byte_order, bpp, bpl, ncolors = h[7], h[11], h[12], h[19]
    masks = h[14:17]
    if bpp != 32 or depth not in (24, 32):
        sys.exit("unsupported xwd: depth %d, %d bits per pixel" % (depth, bpp))
    shifts = [(m & -m).bit_length() - 1 for m in masks]
    fmt = "<" if byte_order == 0 else ">"
    pixels = data[header_size + ncolors * 12:]
    rows, seen = [], set()
    for y in range(height):
        line = pixels[y * bpl:y * bpl + width * 4]
        vals = struct.unpack(fmt + "%dI" % width, line)
        seen.update(vals)
        row = bytearray(b"\x00")
        for v in vals:
            row += bytes(((v & m) >> s) & 0xFF for m, s in zip(masks, shifts))
        rows.append(bytes(row))

    def chunk(tag, body):
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
    open(dst, "wb").write(png)
    print("%s: %dx%d, %d distinct colours" % (dst, width, height, len(seen)))
    if len(seen) < min_colors:
        sys.exit("blank screen? only %d distinct colours (need %d)" % (len(seen), min_colors))


if __name__ == "__main__":
    main()
