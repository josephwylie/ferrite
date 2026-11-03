"""Side-by-sides: a reference capture beside a Ferrite render, both scaled
to 900 px high on a grey ground, each under a label (the sbs2/ layout).

usage: python3 -I sbs.py OUT.jpg LEFT.png "Left label" RIGHT.png "Right label" [crop]

crop, optional: "l,t,r,b/l,t,r,b" as fractions of each image, applied
before scaling (for a sidebar pair, say).
"""
import sys
from PIL import Image, ImageDraw, ImageFont

H, HEAD, GAP, GROUND = 900, 50, 24, (60, 60, 60)
FONT = ImageFont.truetype("/System/Library/Fonts/HelveticaNeue.ttc", 26)


def load(path, crop):
    im = Image.open(path).convert("RGB")
    if crop:
        l, t, r, b = (float(v) for v in crop.split(","))
        w, h = im.size
        im = im.crop((int(l * w), int(t * h), int(r * w), int(b * h)))
    w, h = im.size
    return im.resize((round(w * H / h), H), Image.LANCZOS)


def main():
    out, lpath, llabel, rpath, rlabel = sys.argv[1:6]
    crops = sys.argv[6].split("/") if len(sys.argv) > 6 else [None, None]
    left, right = load(lpath, crops[0]), load(rpath, crops[1])
    sheet = Image.new("RGB", (left.width + GAP + right.width, HEAD + H), GROUND)
    sheet.paste(left, (0, HEAD))
    sheet.paste(right, (left.width + GAP, HEAD))
    draw = ImageDraw.Draw(sheet)
    draw.text((10, 10), llabel, fill=(235, 235, 235), font=FONT)
    draw.text((left.width + GAP + 10, 10), rlabel, fill=(235, 235, 235), font=FONT)
    sheet.save(out, quality=88)


main()
