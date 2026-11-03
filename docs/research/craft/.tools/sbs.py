"""Side-by-sides at true size: 1 pt = 1 px for both images.

Ferrite's visual-reference renders are 2x (1440x900 pt), and the Claude and
Codex captures are 1x, so by default the left (reference) image is kept at
scale 1.0 and the right (Ferrite) image is halved. Never scale both to a
common height: that makes Ferrite look larger than it is. The two sit
top-aligned on a dark ground, each under a label.

usage: python3 -I sbs.py OUT.jpg LEFT.png "Left label" RIGHT.png "Right label"
                         [crop] [--scale L,R]

crop, optional: "l,t,r,b/l,t,r,b" as fractions of each image, applied
before scaling (for a sidebar pair, say). Pass "-" for no crop.
--scale overrides the per-image scales (default 1.0,0.5).
"""
import sys
from PIL import Image, ImageDraw, ImageFont

HEAD, GAP, PAD, GROUND = 44, 24, 12, (8, 8, 8)
FONT = ImageFont.truetype("/System/Library/Fonts/HelveticaNeue.ttc", 22)


def load(path, crop, scale):
    im = Image.open(path).convert("RGB")
    if crop:
        l, t, r, b = (float(v) for v in crop.split(","))
        w, h = im.size
        im = im.crop((int(l * w), int(t * h), int(r * w), int(b * h)))
    if scale != 1.0:
        w, h = im.size
        im = im.resize((round(w * scale), round(h * scale)), Image.LANCZOS)
    return im


def main():
    args = sys.argv[1:]
    scales = (1.0, 0.5)
    if "--scale" in args:
        i = args.index("--scale")
        scales = tuple(float(v) for v in args[i + 1].split(","))
        del args[i : i + 2]
    out, lpath, llabel, rpath, rlabel = args[:5]
    crops = [None, None]
    if len(args) > 5 and args[5] != "-":
        crops = [c or None for c in args[5].split("/")]
    left = load(lpath, crops[0], scales[0])
    right = load(rpath, crops[1], scales[1])
    width = PAD + left.width + GAP + right.width + PAD
    height = HEAD + max(left.height, right.height) + PAD
    sheet = Image.new("RGB", (width, height), GROUND)
    sheet.paste(left, (PAD, HEAD))
    sheet.paste(right, (PAD + left.width + GAP, HEAD))
    draw = ImageDraw.Draw(sheet)
    draw.text((PAD, 12), llabel, fill=(235, 235, 235), font=FONT)
    draw.text((PAD + left.width + GAP, 12), rlabel, fill=(235, 235, 235), font=FONT)
    sheet.save(out, quality=90)


main()
