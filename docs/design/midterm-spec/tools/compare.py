#!/usr/bin/env python3
"""compare.png = reference | mockup side by side, plus diff heat; crops via argv box (phys px)."""
import sys, os
from PIL import Image, ImageChops, ImageDraw
import numpy as np
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ref = Image.open(os.path.join(R, 'ref-window@1.5x.png')).convert('RGB')
mock = Image.open(os.path.join(R, 'mockup@1.5x.png')).convert('RGB').crop((0, 0) + ref.size)
box = tuple(int(v) for v in sys.argv[1:5]) if len(sys.argv) > 4 else (0, 0) + ref.size
out = sys.argv[5] if len(sys.argv) > 5 else os.path.join(R, 'compare.png')
a, b = ref.crop(box), mock.crop(box)
w, h = a.size
canvas = Image.new('RGB', (w * 2 + 12, h + 40), (40, 40, 40))
canvas.paste(a, (0, 40)); canvas.paste(b, (w + 12, 40))
d = ImageDraw.Draw(canvas)
d.text((8, 10), 'REFERENCE (midu, ref-orig.jpg)', fill=(255, 255, 255))
d.text((w + 20, 10), 'MOCKUP (mockup.html @1.5x)', fill=(255, 255, 255))
canvas.save(out)
diff = np.abs(np.asarray(a).astype(int) - np.asarray(b).astype(int)).mean()
print(f'{out} mean abs diff {diff:.2f}')
