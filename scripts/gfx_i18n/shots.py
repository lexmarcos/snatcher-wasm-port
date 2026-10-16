"""before / after pictures from engine frame dumps (work/i18n/gfx)."""
import sys
from PIL import Image
def pair(a, b, out, crop=None, scale=3):
    A = Image.open(a).convert('RGB'); B = Image.open(b).convert('RGB')
    if crop: A = A.crop(crop); B = B.crop(crop)
    W, H = A.size
    im = Image.new('RGB', (W * scale * 2 + 8, H * scale), (255, 0, 255))
    im.paste(A.resize((W * scale, H * scale), Image.NEAREST), (0, 0)); im.paste(B.resize((W * scale, H * scale), Image.NEAREST), (W * scale + 8, 0))
    im.save(out)
if __name__ == '__main__':
    a, b, out = sys.argv[1:4]
    crop = tuple(int(x) for x in sys.argv[4].split(',')) if len(sys.argv) > 4 and sys.argv[4] != '-' else None
    pair(a, b, out, crop, int(sys.argv[5]) if len(sys.argv) > 5 else 3)
