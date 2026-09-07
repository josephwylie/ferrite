#!/usr/bin/env python3
"""Render captured tmux grids; preserve cells/styles and export a local replay viewer.

Requires Pillow and wcwidth. ANSI here is tmux capture-pane -e output, not a general
terminal stream. The raw bidirectional PTY recording remains the source of truth.
"""
import argparse
import base64
import gzip
import html
import json
from functools import lru_cache
from pathlib import Path
import re
import shutil
from PIL import Image, ImageDraw, ImageFont
from wcwidth import wcwidth

DEFAULT_FG = "#d8dee9"
DEFAULT_BG = "#17191f"
ANSI16 = ["#000000", "#800000", "#008000", "#808000", "#000080", "#800080", "#008080", "#c0c0c0",
          "#808080", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff"]
FONTS = [ImageFont.truetype("/System/Library/Fonts/Menlo.ttc", 16, index=i) for i in range(4)]
FALLBACKS = [ImageFont.truetype(p, 16) for p in ["/System/Library/Fonts/Apple Symbols.ttf", "/System/Library/Fonts/Supplemental/Arial Unicode.ttf", "/System/Library/Fonts/STHeiti Light.ttc"]]


@lru_cache(maxsize=4096)
def font_for(text, index):
    for font in [FONTS[index], *FALLBACKS]:
        if bytes(font.getmask(text)) != bytes(font.getmask("\uffff")):
            return font
    return FONTS[index]
ESCAPE = re.compile(r"\x1b\]([^\x07\x1b]*)(?:\x07|\x1b\\)|\x1b\[([0-9:;]*)([A-Za-z])")


def palette(i):
    if i < 16:
        return ANSI16[i]
    if i >= 232:
        return "#" + f"{8 + (i-232)*10:02x}" * 3
    i -= 16
    values = [0, 95, 135, 175, 215, 255]
    return "#" + "".join(f"{values[n]:02x}" for n in [i // 36, i // 6 % 6, i % 6])


def fresh():
    return {"fg": "default", "bg": "default", "bold": False, "dim": False,
            "italic": False, "underline": False, "inverse": False,
            "strike": False, "blink": False, "hidden": False, "link": None}


def sgr(style, sequence, unknown):
    # tmux emits ordinary semicolon colour forms; retain unexpected forms visibly.
    if ":" in sequence:
        unknown.add("SGR:" + sequence)
        return
    codes = [int(x or "0") for x in sequence.split(";")]
    i = 0
    while i < len(codes):
        c = codes[i]
        if c == 0:
            link = style["link"]
            style.update(fresh())
            style["link"] = link
        elif c in [1, 2, 3, 4, 5, 7, 8, 9]:
            style[{1: "bold", 2: "dim", 3: "italic", 4: "underline", 5: "blink", 7: "inverse", 8: "hidden", 9: "strike"}[c]] = True
        elif c == 22:
            style.update(bold=False, dim=False)
        elif c in [23, 24, 25, 27, 28, 29]:
            style[{23: "italic", 24: "underline", 25: "blink", 27: "inverse", 28: "hidden", 29: "strike"}[c]] = False
        elif c in [39, 49]:
            style["fg" if c == 39 else "bg"] = "default"
        elif 30 <= c <= 37 or 90 <= c <= 97:
            style["fg"] = "index:" + str(c - 30 if c < 90 else c - 90 + 8)
        elif 40 <= c <= 47 or 100 <= c <= 107:
            style["bg"] = "index:" + str(c - 40 if c < 100 else c - 100 + 8)
        elif c in [38, 48] and i + 2 < len(codes):
            key = "fg" if c == 38 else "bg"
            if codes[i+1] == 5:
                style[key] = "index:" + str(codes[i+2])
                i += 2
            elif codes[i+1] == 2 and i + 4 < len(codes):
                style[key] = "#" + "".join(f"{v:02x}" for v in codes[i+2:i+5])
                i += 4
            else:
                unknown.add("SGR:" + sequence)
        else:
            unknown.add("SGR:" + str(c))
        i += 1


def parse(frame, styles, style_ids, unknown, history=False):
    cols, rows, cx, cy, cursor, alternate, scrollback, dead = map(int, frame["meta"].split(","))
    source = frame.get("history_ansi", frame["ansi"]) if history else frame["ansi"]
    state = fresh()
    current_style = 0
    grid = []
    for line in source.splitlines():
        cells = []
        pos = 0
        while pos < len(line):
            match = ESCAPE.match(line, pos)
            if match:
                if match[1] is not None:
                    if match[1].startswith("8;"):
                        state["link"] = match[1].split(";", 2)[-1] or None
                    else:
                        unknown.add("OSC:" + match[1])
                elif match[3] == "m":
                    sgr(state, match[2], unknown)
                else:
                    unknown.add("CSI:" + match[0])
                key = json.dumps(state, sort_keys=True)
                if key not in style_ids:
                    style_ids[key] = len(styles)
                    styles.append(state.copy())
                current_style = style_ids[key]
                pos = match.end()
                continue
            ch = line[pos]
            pos += 1
            if ch == "\t":
                # tmux serializes tab cells as HT. Both recorded CLIs retain the
                # terminal's default eight-column tab stops (raw stream audited).
                cells.extend([[" ", current_style, 1] for _ in range(8-len(cells)%8)])
                continue
            width = wcwidth(ch)
            if width < 0:
                unknown.add("control:" + repr(ch))
                continue
            if width == 0:
                previous = len(cells) - 1
                while previous >= 0 and cells[previous][2] == 0:
                    previous -= 1
                if previous >= 0:
                    cells[previous][0] += ch
                continue
            idx = current_style
            cells.append([ch, idx, width])
            if width == 2:
                cells.append(["", idx, 0])
        # capture-pane omits never-written trailing cells. Preserve explicit style
        # on emitted spaces; pad missing cells with the recorded default style.
        if len(cells) > cols:
            unknown.add(f"grid-overflow:{len(cells)}>{cols}")
        cells = cells[:cols]
        cells += [[" ", 0, 1] for _ in range(cols-len(cells))]
        grid.append(cells)
    if not history:
        grid = grid[:rows]
        grid += [[[" ", 0, 1] for _ in range(cols)] for _ in range(rows-len(grid))]
    return {"time": frame["time"], "columns": cols, "rows": len(grid),
            "cursor": [cx, cy, bool(cursor)], "alternate": bool(alternate),
            "history_rows": scrollback, "label": frame.get("label"), "cells": grid}


def colour(value, background=False):
    if value == "default":
        return DEFAULT_BG if background else DEFAULT_FG
    if value.startswith("index:"):
        return palette(int(value[6:]))
    return value


def rgb(value):
    return tuple(int(value[i:i+2], 16) for i in [1, 3, 5])


def draw_frame(frame, styles, destination, spaces=False):
    cw, rh, pad = 10, 22, 20
    im = Image.new("RGB", (frame["columns"]*cw+pad*2, frame["rows"]*rh+pad*2), DEFAULT_BG)
    draw = ImageDraw.Draw(im)
    for y, row in enumerate(frame["cells"]):
        for x, (ch, idx, width) in enumerate(row):
            style = styles[idx]
            fg, bg = colour(style["fg"]), colour(style["bg"], True)
            if style["inverse"]:
                fg, bg = bg, fg
            draw.rectangle((pad+x*cw, pad+y*rh, pad+(x+1)*cw-1, pad+(y+1)*rh-1), fill=bg)
    for y, row in enumerate(frame["cells"]):
        for x, (ch, idx, width) in enumerate(row):
            if width == 0:
                continue
            style = styles[idx]
            fg, bg = colour(style["fg"]), colour(style["bg"], True)
            if style["inverse"]:
                fg, bg = bg, fg
            if style["dim"]:
                fg = tuple(round(a*.5+b*.5) for a, b in zip(rgb(fg), rgb(bg)))
            if style["hidden"]:
                ch = " "
            if spaces and ch in [" ", "\u00a0"]:
                ch, fg = ("·" if ch == " " else "⍽"), "#525966"
            draw.text((pad+x*cw, pad+y*rh+17), ch, fill=fg,
                      font=font_for(ch, int(style["bold"]) + 2*int(style["italic"])), anchor="ls")
            if style["underline"]:
                draw.line((pad+x*cw, pad+y*rh+19, pad+(x+width)*cw-1, pad+y*rh+19), fill=fg)
            if style["strike"]:
                draw.line((pad+x*cw, pad+y*rh+11, pad+(x+width)*cw-1, pad+y*rh+11), fill=fg)
    cx, cy, visible = frame["cursor"]
    if visible and 0 <= cy < frame["rows"]:
        draw.rectangle((pad+cx*cw, pad+cy*rh, pad+(cx+1)*cw-1, pad+(cy+1)*rh-1), outline="#d8dee9")
    im.save(destination)


VIEWER = r'''<!doctype html><meta charset="utf-8"><title>Ferrite CLI evidence</title>
<style>@font-face{font-family:CaptureSymbols;src:url(data:font/ttf;base64,__FONT__)}body{margin:20px;background:#111318;color:#d8dee9;font:14px system-ui}button,select,input{margin:4px;padding:6px;background:#232831;color:inherit;border:1px solid #515765}canvas{display:block;margin:16px 0;max-width:100%;height:auto}pre{white-space:pre-wrap}#inspector{position:sticky;bottom:0;padding:12px;background:#252b36}label{margin-right:12px}</style>
<h1>CLI transcript evidence</h1><p>Reconstructed terminal cells. Not native screenshots. Coordinates are zero-based. Hover a cell for exact Unicode and styling.</p>
<select id="provider"></select><select id="marks"></select><button id="previous">Previous</button><button id="play">Play</button><button id="next">Next</button>
<label><input type="checkbox" id="spaces">Show spaces</label><label><input type="checkbox" id="grid">Grid</label>
<input id="slider" type="range" min="0" style="width:95%"><pre id="info"></pre><canvas id="canvas"></canvas><pre id="inspector">Hover a cell.</pre>
<script>
const compressed='__DATA__';
const bytes=Uint8Array.from(atob(compressed),c=>c.charCodeAt(0));
new Response(new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'))).json().then(data=>{
const $=id=>document.getElementById(id),cv=$('canvas'),ctx=cv.getContext('2d'),cw=10,rh=22,pad=20;
let frames=[],current=0,timer=null;
const palette=__PALETTE__;
function color(c,bg=false){if(c==='default')return bg?'#17191f':'#d8dee9';if(c.startsWith('index:'))return palette[+c.slice(6)];return c}
function frame(){let f=frames[current];if(!f)return;return {...f,cells:f.row_ids.map(i=>data.row_pool[i])}}
function draw(){const f=frame();if(!f)return;cv.width=f.columns*cw+pad*2;cv.height=f.rows*rh+pad*2;ctx.fillStyle='#17191f';ctx.fillRect(0,0,cv.width,cv.height);
for(let y=0;y<f.rows;y++)for(let x=0;x<f.columns;x++){let [ch,si,w]=f.cells[y][x],s=data.styles[si],fg=color(s.fg),bg=color(s.bg,true);if(s.inverse)[fg,bg]=[bg,fg];ctx.fillStyle=bg;ctx.fillRect(pad+x*cw,pad+y*rh,cw,rh)}
for(let y=0;y<f.rows;y++)for(let x=0;x<f.columns;x++){let [ch,si,w]=f.cells[y][x],s=data.styles[si];if(!w)continue;let fg=color(s.fg),bg=color(s.bg,true);if(s.inverse)[fg,bg]=[bg,fg];ctx.globalAlpha=s.dim?.5:1;ctx.fillStyle=fg;if(s.hidden)ch=' ';if($('spaces').checked&&(ch===' '||ch==='\u00a0')){ch=ch===' '?'·':'⍽';ctx.fillStyle='#525966'}ctx.font=`${s.italic?'italic ':''}${s.bold?'bold ':''}16px Menlo,CaptureSymbols,monospace`;ctx.fillText(ch,pad+x*cw,pad+y*rh+17);if(s.underline)ctx.fillRect(pad+x*cw,pad+y*rh+19,cw*w,1);if(s.strike)ctx.fillRect(pad+x*cw,pad+y*rh+11,cw*w,1);ctx.globalAlpha=1}
if($('grid').checked){ctx.strokeStyle='#46506266';ctx.lineWidth=.5;for(let x=0;x<=f.columns;x++){ctx.beginPath();ctx.moveTo(pad+x*cw,pad);ctx.lineTo(pad+x*cw,pad+f.rows*rh);ctx.stroke()}for(let y=0;y<=f.rows;y++){ctx.beginPath();ctx.moveTo(pad,pad+y*rh);ctx.lineTo(pad+f.columns*cw,pad+y*rh);ctx.stroke()}}
if(f.cursor[2]){ctx.strokeStyle='#d8dee9';ctx.strokeRect(pad+f.cursor[0]*cw,pad+f.cursor[1]*rh,cw,rh)}$('slider').value=current;$('info').textContent=`${$('provider').value} · frame ${current+1}/${frames.length} · ${(f.time-frames[0].time).toFixed(3)}s · ${f.columns}×${f.rows} · ${f.alternate?'alternate screen':'normal screen'}${f.label?' · '+f.label:''}`;}
function selectProvider(){if(timer){clearTimeout(timer);timer=null;$('play').textContent='Play'}frames=data.providers[$('provider').value];current=Math.max(0,frames.findIndex(f=>f.label==='02-read-complete'));$('slider').max=frames.length-1;$('marks').innerHTML='<option value="">Jump to labelled state</option>';frames.forEach((f,i)=>{if(f.label){let o=new Option(f.label,i);$('marks').add(o)}});$('marks').value=current;draw()}
Object.keys(data.providers).forEach(n=>$('provider').add(new Option(n,n)));$('provider').onchange=selectProvider;$('slider').oninput=()=>{current=+$('slider').value;draw()};$('marks').onchange=()=>{if($('marks').value!==''){current=+$('marks').value;draw()}};$('previous').onclick=()=>{current=Math.max(0,current-1);draw()};$('next').onclick=()=>{current=Math.min(frames.length-1,current+1);draw()};$('spaces').onchange=$('grid').onchange=draw;
$('play').onclick=()=>{if(timer){clearTimeout(timer);timer=null;$('play').textContent='Play';return}$('play').textContent='Pause';function tick(){if(current>=frames.length-1){timer=null;$('play').textContent='Play';return}const delay=Math.max(20,(frames[current+1].time-frames[current].time)*1000);timer=setTimeout(()=>{current++;draw();tick()},delay)}tick()};
cv.onmousemove=e=>{let rect=cv.getBoundingClientRect(),x=Math.floor(((e.clientX-rect.left)*cv.width/rect.width-pad)/cw),y=Math.floor(((e.clientY-rect.top)*cv.height/rect.height-pad)/rh),f=frame();if(x<0||y<0||x>=f.columns||y>=f.rows)return;let [ch,si,w]=f.cells[y][x];$('inspector').textContent=`row ${y}, column ${x} · ${JSON.stringify(ch)} · ${[...ch].map(c=>'U+'+c.codePointAt(0).toString(16).toUpperCase().padStart(4,'0')).join(' ')} · width ${w}\n${JSON.stringify(data.styles[si])}`};selectProvider();document.fonts.load('16px CaptureSymbols').then(draw);
});</script>'''


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("root", type=Path)
    args = p.parse_args()
    root = args.root.resolve()
    symbol_font = root / "fonts" / "NotoSansSymbols2-Regular.ttf"
    if not symbol_font.exists():
        raise SystemExit("Missing fonts/NotoSansSymbols2-Regular.ttf; see capture README for source and licence.")
    FALLBACKS.append(ImageFont.truetype(str(symbol_font), 16))
    font_for.cache_clear()
    styles = [fresh()]
    ids = {json.dumps(styles[0], sort_keys=True): 0}
    unknown = set()
    providers = {}
    summaries = {}
    excluded = []
    for name in ["claude", "codex"]:
        directory = root / name
        rendered = directory / "rendered"
        rendered.mkdir(exist_ok=True)
        marks = []
        for path in sorted(directory.glob("*.json")):
            raw = json.loads(path.read_text())
            if "label" not in raw:
                continue
            f = parse(raw, styles, ids, unknown)
            marks.append(f)
            draw_frame(f, styles, rendered / (path.stem + ".png"))
            draw_frame(f, styles, rendered / (path.stem + "-spaces.png"), spaces=True)
            (rendered / (path.stem + "-cells.json")).write_text(json.dumps(f, ensure_ascii=False))
            lines = ["".join(c[0] for c in row) for row in f["cells"]]
            (rendered / (path.stem + ".txt")).write_text("\n".join(lines) + "\n")
            summaries[name + "/" + path.stem] = []
            for y, row in enumerate(f["cells"]):
                occupied = [x for x,c in enumerate(row) if c[0] != " "]
                if not occupied:
                    continue
                end = occupied[-1] + 1
                # Use actual cell indices: wide glyphs and combining marks must
                # never turn this into codepoint-counted spacing.
                spaces = "".join(" " if c[0] == " " else "x" for c in row[:end])
                summaries[name + "/" + path.stem].append({
                    "row": y, "leading_spaces": occupied[0], "text_columns": end,
                    "space_runs": [[m.start(), len(m[0])] for m in re.finditer(" +", spaces)],
                    "visible": lines[y].rstrip(" ").replace(" ", "·").replace("\u00a0", "⍽")})
        frames = []
        for line in (directory / "frames.jsonl").read_text().splitlines():
            raw = json.loads(line)
            frame_unknown = set()
            f = parse(raw, styles, ids, frame_unknown)
            if any(s.startswith("grid-overflow:") for s in frame_unknown):
                excluded.append({"provider": name, "index": raw["index"], "time": raw["time"], "reason": sorted(frame_unknown)})
                continue
            unknown.update(frame_unknown)
            frames.append(f)
        providers[name] = sorted(frames + marks, key=lambda f:f["time"])
    # A screen usually changes only one or two rows. Deduplicate rows so replay
    # does not allocate millions of repeated JavaScript cell arrays at startup.
    row_pool, row_ids = [], {}
    for frames in providers.values():
        for f in frames:
            rows = f.pop("cells")
            f["row_ids"] = []
            for row in rows:
                key = json.dumps(row, ensure_ascii=False, separators=(",", ":"))
                if key not in row_ids:
                    row_ids[key] = len(row_pool)
                    row_pool.append(row)
                f["row_ids"].append(row_ids[key])
    data = {"styles": styles, "row_pool": row_pool, "providers": providers}
    packed = gzip.compress(json.dumps(data, ensure_ascii=False, separators=(",", ":")).encode())
    (root / "grids.json.gz").write_bytes(packed)
    (root / "styles.json").write_text(json.dumps(styles, indent=2))
    (root / "spacing-audit.json").write_text(json.dumps(summaries, ensure_ascii=False, indent=2))
    (root / "viewer.html").write_text(VIEWER.replace("__DATA__", base64.b64encode(packed).decode()).replace("__PALETTE__", json.dumps([palette(i) for i in range(256)])).replace("__FONT__", base64.b64encode(symbol_font.read_bytes()).decode()))
    manifest = json.loads((root / "manifest.json").read_text())
    fixture = Path(manifest["fixture"])
    (root / "fixture").mkdir(exist_ok=True)
    for filename in ["sample.py", "sample.txt"]:
        shutil.copy2(fixture / filename, root / "fixture" / filename)
    quality = {"unknown_sequences": sorted(unknown), "style_count": len(styles),
               "excluded_resize_race_frames": excluded,
               "frame_counts": {k: len(v) for k,v in providers.items()}, "packed_grid_bytes": len(packed),
               "render": {"font": "Menlo", "size_px": 16, "cell_width_px": 10, "row_height_px": 22, "padding_px": 20,
                          "dim_opacity": 0.5, "palette_0_15": "xterm reference palette (reconstruction choice)",
                          "cursor": "outline reconstruction; actual cursor shape not derived"}}
    (root / "render-quality.json").write_text(json.dumps(quality, indent=2))
    print(json.dumps(quality, indent=2))


if __name__ == "__main__":
    main()
