"""Compares two folders of UI screenshots made by ui_shots.ps1.

Ignores what is not deterministic: the resolution/fps label in the pill (it is
measured live) and the two views that show the PC's real windows or live
numbers (sources, stats). Everything else must match pixel for pixel.

Needs Pillow and numpy (pip install pillow numpy).

Usage: python tools/ui_compare.py BEFORE_DIR AFTER_DIR
Exit code 0 when nothing differs, 1 otherwise.
"""
import os
import sys

import numpy as np
from PIL import Image

SKIP = ("sources", "stats")
MASK = (290, 360, 650, 680)  # x0, x1, y0, y1 of the resolution/fps label at 1280x720


def load(folder, name):
    return np.asarray(Image.open(os.path.join(folder, name)).convert("RGBA"), dtype=np.int16)


def main(a_dir, b_dir):
    bad = checked = 0
    for name in sorted(os.listdir(a_dir)):
        if not name.endswith(".png") or any(s in name for s in SKIP):
            continue
        a, b = load(a_dir, name), load(b_dir, name)
        checked += 1
        if a.shape != b.shape:
            print(f"{name}: SIZE {a.shape} -> {b.shape}")
            bad += 1
            continue
        d = (a != b).any(axis=2)
        x0, x1, y0, y1 = MASK
        d[y0:y1, x0:x1] = False
        n = int(d.sum())
        if n:
            ys, xs = np.where(d)
            print(f"{name}: DIFFERENT {n} px, bbox x {xs.min()}-{xs.max()} y {ys.min()}-{ys.max()}")
            bad += 1
        else:
            print(f"{name}: same")
    print(f"{checked} compared, {bad} different")
    return 1 if bad else 0


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    sys.exit(main(sys.argv[1], sys.argv[2]))
