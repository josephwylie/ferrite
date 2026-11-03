# Claude Desktop working glyph: decoded

Source: Claude Desktop's `ion-dist.asar` bundle (extracted 2026-11-03). Every number below comes from the bundle's own assets, not from a screen recording. The recording is used only for the cross-check at the end.

## Where it lives and how it works

The glyph is **not** drawn by a CSS keyframe, an SVG, Lottie, or a requestAnimationFrame loop. It is a **pre-rendered raster flipbook**: a vertical strip of 48×48 WebP frames. Claude steps through the frames with WAAPI `translateY` and `steps(n, jump-none)`, at **33.333 ms per frame (30 fps)**.

| Piece | File (assets/v1/) | What it does |
|---|---|---|
| Turn status, Code view | `c96d752d0-*.js`: `W6` → `U6` → `ddt` → `nM(...)` → `$w` | Renders `<uf mark="clay" activity={Cve() ?? "think"}>`. |
| Mark wrapper | `c05970c42-*.js` | 20×20 box. State `working` / `blocked` / `tile`. |
| `WorkingMark` (`bO`/`fO`) | `shared-5-*.js` (`EO`, `data-cds="WorkingMark"`) | Box `yO = 20` px, colour `var(--cds-clay)`. Before the sheets load, it shows a 0.35×20 px dot. Its state machine runs `grow` → `<body>.in` → `<body>.loop` (repeating) → `<body>.out`. |
| Sheets + timing | `c8677db5f-*.js` (`cds_working_mark_sheets`) | `SHEETS` (frames, speed, seams, rest), `SHEET_FRAME = 48`, `SHEET_URLS`. |
| The four-dot loop | `c05229fc6-D3jyHLR7.webp` (`code.loop`) | 48×3600 px, i.e. 75 frames. |

**Which body plays.** The body is chosen by tool activity (`c77e464d3-*.js`, `lt()`):

- `bash`, `code`, `kill_bash`, `tmux`, or any tool name containing exec/run/shell/python/… → **`code`**, which is the four dots;
- read/grep/glob → `read`;
- write/edit/todo → `write`;
- web → `search`;
- otherwise `think` or `default`.

So the square/diagonal four-dot glyph is specifically **`code.loop`**, which shows while a shell-like tool runs. The other bodies are different glyphs.

Minimal excerpt (minified, abridged):

```js
// c8677db5f: timing table + sheet url
"code.in":{frames:16,speed:33.3333},
"code.loop":{frames:75,speed:33.3333,seams:[18,37,56],rest:74},
"code.out":{frames:9,speed:33.3333},
a=new URL("/assets/v1/c05229fc6-D3jyHLR7.webp",import.meta.url).href, E=48 /*SHEET_FRAME*/
// shared-5: one stage = one WAAPI flipbook pass
function aO(e,t){return`translateY(${-t*e}px)`}
function oO(e,t,n){return Array.from({length:n},(n,r)=>({transform:aO(e,t+r)}))}
i=on(e,oO(c,t,r),{duration:K*r,iterations:1,
  easing:r>1?`steps(${r}, jump-none)`:"linear",fill:"forwards"});
i.finished.then(()=>{ let e=dO(S,M.current); e===S?T(hO):C(e) })  // same body → replay loop
// loop→out only at a seam (frame*speed) or after ≥2 s for default/blocked/sphere
var yO=20 /*px box*/, vO=.35 /*pre-load dot = 0.35*20 px*/
```

**Looping.** Each pass is `iterations: 1`. When it finishes, React re-arms the same stage. A pass takes exactly 75 × 33.333 = **2500 ms**. The recording shows that a re-arm can slip by a frame or two (about 50 ms in one of four loops).

**Seams.** Seams at frames 18, 37 and 56 are the frames where a body change may cut to `code.out`.

**Rest frame.** Frame 74 is the `rest` pose, a perfect square. It is used for reduced motion. In the normal loop it replaces one frame of the continuous motion, so Claude shows a ≤1 px snap at the loop seam once every 2.5 s.

## Decoded motion (code.loop)

I fitted each sprite frame with four anti-aliased discs (x, y, r) by least squares. The RMS alpha error was about 0.02, which is rasterisation-limited, and the fit tracks the dots through the frames where they merge.

**Geometry.** Units are fractions of the glyph box, so ×20 px gives Claude's on-screen size.

- **Rest:** a square. Dot centres sit at 0.5 ± **0.2481** (11.91 px of 48), which puts them **0.3508** from the centre.
- **Dot radius:** **0.0949** (diameter 0.19, or 3.8 px at 20 px).
- **Colour:** `#D97757` (217,119,87), opaque, no opacity animation.

**Structure.** The loop is **four identical "pair swaps", one every 625 ms** (18.75 frames). All four mid-points were measured to ±0.02 frame: 7.70, 26.47, 45.20 and 63.93 (period 75).

- Swaps 1 and 3 move the **TL/BR pair** **counter-clockwise** on screen.
- Swaps 2 and 4 move the **TR/BL pair** **clockwise**.
- The other pair stays still.

**How a swap moves.** In each swap, the two dots exchange corners by turning 180° about the centre while pulling inward:

- The radius dips from 0.351 to about 0.155 (front) or 0.138 (back).
- Both dots share the same angle progress. The angle eases in slowly, whips through the middle, and has a long ease-out tail, so each dot glides into its corner along the outer circle. That is why a dot briefly overshoots the square's edge by up to 0.067 (front) or 0.054 (back).
- At mid-swap (u = 0.5), the moving pair lies on the *other* diagonal, so all four dots form a line: `/` during TL/BR swaps and `\` during TR/BL swaps.
- So the glyph reads as square → `/` → square → `\` → … with one shape change every 625 ms.

**Depth cue (scale).** The dot that starts on the right-hand side (BR or TR), the "front" dot, swells to 1.053× its radius and sits slightly further out. The "back" dot shrinks to 0.943×. The ratios match a perspective factor of about ±6% at mid-swap. There are no per-dot delays beyond this front/back asymmetry: both dots of a pair start and end together.

**Per-swap timing.** Motion is visible from τ ≈ −10 frames (−333 ms) to τ ≈ +12 frames (+400 ms) around the swap mid-point. Consecutive swaps therefore overlap: the next pair starts moving while the last pair is still settling.

**Canonical swap.** All four swaps are pooled. Their mid-points fall at four different sub-frame phases, which gives an effective 120 Hz sampling. The pooled data is smoothed with a smoothing spline.

How to read the table:

- `a` is the distance along the start-corner direction v̂ (from centre to the start corner).
- `b` is the distance along p̂, which is v̂ rotated 90° toward the direction of travel (CCW for TL/BR, CW for TR/BL).
- `r` is the dot radius.
- τ = 0 at u = 0.5.
- Position = (0.5, 0.5) + a·v̂ + b·p̂.

| τ (frames) | τ (ms) | front a | front b | front r | back a | back b | back r |
|---:|---:|---:|---:|---:|---:|---:|---:|
| −18.75 | −625 | +0.3508 | +0.0000 | 0.0949 | +0.3508 | +0.0000 | 0.0949 |
| −12 | −400 | +0.3521 | +0.0004 | 0.0952 | +0.3497 | +0.0003 | 0.0939 |
| −10 | −333 | +0.3511 | +0.0018 | 0.0958 | +0.3453 | +0.0016 | 0.0935 |
| −8 | −267 | +0.3445 | +0.0058 | 0.0967 | +0.3340 | +0.0047 | 0.0931 |
| −7 | −233 | +0.3374 | +0.0103 | 0.0972 | +0.3238 | +0.0088 | 0.0927 |
| −6 | −200 | +0.3256 | +0.0174 | 0.0978 | +0.3086 | +0.0154 | 0.0923 |
| −5 | −167 | +0.3067 | +0.0282 | 0.0984 | +0.2866 | +0.0250 | 0.0917 |
| −4 | −133 | +0.2773 | +0.0438 | 0.0990 | +0.2551 | +0.0388 | 0.0910 |
| −3 | −100 | +0.2334 | +0.0649 | 0.0995 | +0.2120 | +0.0575 | 0.0904 |
| −2 | −67 | +0.1717 | +0.0918 | 0.0999 | +0.1552 | +0.0817 | 0.0898 |
| −1.5 | −50 | +0.1337 | +0.1074 | 0.1000 | +0.1211 | +0.0955 | 0.0896 |
| −1 | −33 | +0.0914 | +0.1237 | 0.1000 | +0.0832 | +0.1099 | 0.0895 |
| −0.5 | −17 | +0.0453 | +0.1399 | 0.1000 | +0.0420 | +0.1243 | 0.0894 |
| 0 | 0 | −0.0033 | +0.1548 | 0.0999 | −0.0017 | +0.1377 | 0.0895 |
| +0.5 | +17 | −0.0531 | +0.1674 | 0.0997 | −0.0467 | +0.1494 | 0.0896 |
| +1 | +33 | −0.1021 | +0.1766 | 0.0995 | −0.0916 | +0.1584 | 0.0898 |
| +1.5 | +50 | −0.1485 | +0.1816 | 0.0992 | −0.1350 | +0.1641 | 0.0901 |
| +2 | +67 | −0.1908 | +0.1822 | 0.0989 | −0.1754 | +0.1660 | 0.0905 |
| +3 | +100 | −0.2588 | +0.1700 | 0.0980 | −0.2427 | +0.1579 | 0.0914 |
| +4 | +133 | −0.3035 | +0.1449 | 0.0971 | −0.2894 | +0.1368 | 0.0923 |
| +5 | +167 | −0.3286 | +0.1152 | 0.0963 | −0.3184 | +0.1099 | 0.0931 |
| +6 | +200 | −0.3412 | +0.0869 | 0.0958 | −0.3346 | +0.0830 | 0.0938 |
| +8 | +267 | −0.3497 | +0.0441 | 0.0951 | −0.3472 | +0.0414 | 0.0946 |
| +10 | +333 | −0.3508 | +0.0197 | 0.0950 | −0.3510 | +0.0179 | 0.0951 |
| +12 | +400 | −0.3508 | +0.0082 | 0.0951 | −0.3516 | +0.0065 | 0.0952 |
| +15 | +500 | −0.3509 | +0.0019 | 0.0951 | −0.3516 | +0.0004 | 0.0950 |
| +18.75 | +625 | −0.3508 | +0.0000 | 0.0949 | −0.3508 | +0.0000 | 0.0949 |

**Loop schedule.** Times are in ms within the 2500 ms loop, in the same phase as Claude's frame 0.

| Swap | Mid-point | Pair | Direction | Line at mid |
|---|---:|---|---|---|
| 1 | 256.7 | TL↔BR | CCW | `/` |
| 2 | 881.7 | TR↔BL | CW | `\` |
| 3 | 1506.7 | TL↔BR | CCW | `/` |
| 4 | 2131.7 | TR↔BL | CW | `\` |

**Easing.** The source contains no easing curve, only rendered frames. The authoring tool's curves are not in the bundle.

- As a compact stand-in, the shared angle progress u(τ) fits `cubic-bezier(0.605, 0, 0.016, 1)` over 23.2 frames (774 ms) starting at τ = −8.36 frames. The error is 0.53° RMS (1.85° max).
- For faithful playback, use the table above with cubic interpolation, or the spline in the renderer. Do not use the bezier.

## Re-render and check

Scratchpad: `/private/tmp/claude-501/-Users-josephwylie-Desktop-Projects-ferrite/1ddcaa61-e930-48fc-bcad-ca713746eedf/scratchpad/claude-orbs/`

| Script | Purpose |
|---|---|
| `analysis/fit.py`, `analysis/canon.py` | Sprite → per-frame discs → canonical swap samples |
| `spec.py` | Decoded motion as splines; `dots(t)` |
| `render_orbs.py` | Four white orbs on #191919, 60 fps, analytic positions |
| `selfcheck_fit.py`, `selfcheck_render.py` | Checks against the sprite and the recording |

Checks:

- **Spec vs sprite (the real source).** Rendered at 48 px, the spec has an RMS alpha error of 0.027 per frame, against 0.021 for an unconstrained per-frame fit. Centres are within 0.11 px RMS (0.26 max) of 48 px, which is ≤0.05 px at Claude's 20 px size.
- **Spec vs recording.** The crop is `claude-stream.mp4` at 40×40 @ (714, 862), 1× pixels, glyph box fitted at 19.6 px. The spec is stepped at 30 fps exactly as Claude steps it, and a phase is fitted per loop.
  - Per-frame correlation: median **0.909**, 5th percentile 0.847.
  - Control: the same spec shifted by half a swap (312 ms) scores median 0.478.
  - The worst frames sit at one loop re-arm, where Claude slipped about 50 ms.
  - Visual check: `out/selfcheck-recording.mp4` (recording | spec @30 stepped | spec smooth) and `out/selfcheck-recording-strip.png`.
