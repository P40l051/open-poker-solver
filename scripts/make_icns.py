#!/usr/bin/env python3
"""Builds assets/icon.icns from assets/icon.png without relying on iconutil.

macOS accepts PNG-compressed ICNS chunks, so the master is resized with sips
and packed into the standard element table.
"""
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MASTER = ROOT / "assets" / "icon.png"
OUT = ROOT / "assets" / "icon.icns"

# element type -> pixel size
ELEMENTS = {
    "ic07": 128,
    "ic08": 256,
    "ic09": 512,
    "ic10": 1024,
    "ic11": 32,   # 16x16@2x
    "ic12": 64,   # 32x32@2x
    "ic13": 256,  # 128x128@2x
    "ic14": 512,  # 256x256@2x
}
# Small legacy 1x slots (32-bit RGB) are optional; modern macOS uses the PNG
# chunks above plus the 1x sizes below.
SIZES = {
    "is32": 16,
    "il32": 32,
    "ih32": 48,
    "it32": 128,
}


def resize(src: Path, size: int, tmp: Path) -> bytes:
    dst = tmp / f"icon_{size}.png"
    subprocess.run(
        ["sips", "-z", str(size), str(size), str(src), "--out", str(dst)],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return dst.read_bytes()


def main() -> int:
    if not MASTER.exists():
        print(f"missing {MASTER}; run scripts/make_icon.swift first", file=sys.stderr)
        return 1

    chunks = []
    with tempfile.TemporaryDirectory() as tdir:
        tmp = Path(tdir)
        for typ, size in ELEMENTS.items():
            data = resize(MASTER, size, tmp)
            chunks.append(typ.encode("ascii") + struct.pack(">I", len(data) + 8) + data)

    body = b"".join(chunks)
    icns = b"icns" + struct.pack(">I", len(body) + 8) + body
    OUT.write_bytes(icns)
    print(f"wrote {OUT} ({len(icns)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
