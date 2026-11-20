"""Generate the README graphics for Shepherd (assets/readme/*.svg).

Usage: python3 scripts/readme/gen_assets.py assets/readme

Brand tokens follow the desktop redesign: #07070A background, the grey pixel flock (native/app/src/logo.rs),
the prism ramp and wave (a port of native/app/src/prism.rs), JetBrains Mono for text, a Silkscreen-like
pixel face drawn as cells for labels. Static: no animation.
"""

import math
import sys
from collections import defaultdict
from pathlib import Path
from xml.sax.saxutils import escape

OUT = Path(sys.argv[1] if len(sys.argv) > 1 else "out")
OUT.mkdir(parents=True, exist_ok=True)

BG = "#07070A"
CARD = "#0E0E14"
LINE = "#23232E"
INK = "#F4F4F8"
MUTED = "#8A8A98"
DIM = "#5A5A6A"
VIOLET = "#B98CFF"
ORANGE = "#FF8A1F"
CYAN = "#19D3FF"
MONO = "'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, 'Liberation Mono', monospace"

# ---------------------------------------------------------------- cells -> compact paths


class Cells:
    """Collects square cells and writes one <path> per colour."""

    def __init__(self):
        self.by_color = defaultdict(list)

    def add(self, x, y, w, h, color):
        self.by_color[color].append((x, y, w, h))

    def svg(self, extra=""):
        out = []
        for color, rects in self.by_color.items():
            d = "".join(f"M{fmt(x)} {fmt(y)}h{fmt(w)}v{fmt(h)}h{fmt(-w)}z" for x, y, w, h in rects)
            out.append(f'<path fill="{color}" d="{d}"{extra}/>')
        return "\n".join(out)


def fmt(v):
    v = round(v, 2)
    return str(int(v)) if v == int(v) else str(v)


# ---------------------------------------------------------------- the flock (logo.rs)

SHEEP = [".WW.WW..", "WWWWWWHE", "WWWWWWHH", "WWWWWW..", ".L..L..."]
HERD = [(0, 0, 0), (10, 0, 1), (5, 4, 2)]


def tone(t, c):
    table = {
        (0, "W"): "#4A4A5A", (0, "E"): BG, (0, "_"): "#3A3A48",
        (1, "W"): "#8A8A98", (1, "E"): BG, (1, "_"): "#6A6A78",
        (2, "W"): "#F4F4F8", (2, "H"): "#B4B4C0", (2, "E"): BG, (2, "_"): "#9A9AA8",
    }
    return table.get((t, c)) or table[(t, "_")]


def flock(cells, ox, oy, u):
    for x0, y0, t in HERD:
        for ry, row in enumerate(SHEEP):
            for rx, ch in enumerate(row):
                if ch != ".":
                    cells.add(ox + (x0 + rx) * u, oy + (y0 + ry) * u, u, u, tone(t, ch))


# ---------------------------------------------------------------- pixel face (Silkscreen-like)

GLYPHS = {
    "A": [".##.", "#..#", "####", "#..#", "#..#"],
    "B": ["###.", "#..#", "###.", "#..#", "###."],
    "C": [".###", "#...", "#...", "#...", ".###"],
    "D": ["###.", "#..#", "#..#", "#..#", "###."],
    "E": ["####", "#...", "###.", "#...", "####"],
    "F": ["####", "#...", "###.", "#...", "#..."],
    "G": [".###", "#...", "#.##", "#..#", ".###"],
    "H": ["#..#", "#..#", "####", "#..#", "#..#"],
    "I": ["###", ".#.", ".#.", ".#.", "###"],
    "J": ["..##", "...#", "...#", "#..#", ".##."],
    "K": ["#..#", "#.#.", "##..", "#.#.", "#..#"],
    "L": ["#...", "#...", "#...", "#...", "####"],
    "M": ["#...#", "##.##", "#.#.#", "#...#", "#...#"],
    "N": ["#..#", "##.#", "#.##", "#..#", "#..#"],
    "O": [".##.", "#..#", "#..#", "#..#", ".##."],
    "P": ["###.", "#..#", "###.", "#...", "#..."],
    "Q": [".##.", "#..#", "#..#", "#.#.", ".#.#"],
    "R": ["###.", "#..#", "###.", "#.#.", "#..#"],
    "S": [".###", "#...", ".##.", "...#", "###."],
    "T": ["###", ".#.", ".#.", ".#.", ".#."],
    "U": ["#..#", "#..#", "#..#", "#..#", ".##."],
    "V": ["#...#", "#...#", ".#.#.", ".#.#.", "..#.."],
    "W": ["#...#", "#...#", "#.#.#", "##.##", "#...#"],
    "X": ["#..#", "#..#", ".##.", "#..#", "#..#"],
    "Y": ["#.#", "#.#", ".#.", ".#.", ".#."],
    "Z": ["####", "..#.", ".#..", "#...", "####"],
    "0": [".##.", "#..#", "#..#", "#..#", ".##."],
    "1": [".#", "##", ".#", ".#", ".#"],
    "2": ["###.", "...#", ".##.", "#...", "####"],
    "3": ["###.", "...#", ".##.", "...#", "###."],
    "4": ["#..#", "#..#", "####", "...#", "...#"],
    "5": ["####", "#...", "###.", "...#", "###."],
    "6": [".##.", "#...", "###.", "#..#", ".##."],
    "7": ["####", "...#", "..#.", ".#..", ".#.."],
    "8": [".##.", "#..#", ".##.", "#..#", ".##."],
    "9": [".##.", "#..#", ".###", "...#", ".##."],
    "-": ["...", "...", "###", "...", "..."],
    ".": [".", ".", ".", ".", "#"],
    " ": ["..", "..", "..", "..", ".."],
}


def pixel_width(text, u, gap=1):
    w = 0
    for ch in text.upper():
        w += (len(GLYPHS[ch][0]) + gap) * u
    return w - gap * u


def pixel_text(cells, text, ox, oy, u, color, gap=1):
    x = ox
    for ch in text.upper():
        g = GLYPHS[ch]
        for ry, row in enumerate(g):
            for rx, c in enumerate(row):
                if c == "#":
                    cells.add(x + rx * u, oy + ry * u, u, u, color)
        x += (len(g[0]) + gap) * u
    return x


# ---------------------------------------------------------------- the prism (prism.rs)

RAMP = [
    (244, 244, 246), (250, 230, 0), (248, 192, 0), (248, 154, 0), (248, 58, 0), (248, 42, 80),
    (250, 0, 160), (220, 0, 204), (100, 40, 204), (56, 24, 208), (42, 10, 160), (0, 52, 248),
    (42, 120, 248), (0, 192, 248),
]
WARM = 7
BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5]
LOOP = 12


def h(a, b, c):
    n = math.sin(a * 127.1 + b * 311.7 + c * 74.7) * 43758.5453
    return n - math.floor(n)


def prism_cells(width, height, p, frame):
    out = []
    px = p["px"]
    cols = math.ceil(width / px)
    rows = math.ceil(height / px)
    fr = frame % LOOP
    t = fr / LOOP * math.tau
    reach = max(cols * p["reach"], 2.0)

    def paint(x, y, hue, m):
        last = len(RAMP) - 1
        hh = min(max(hue, 0.0), 0.999) * last
        i = math.floor(hh)
        k = round((hh - i) * 4) / 4  # quantised mix: visually the same, far fewer colours
        a, b = RAMP[i], RAMP[min(i + 1, last)]
        if m < 1.0 and i < WARM:
            rgb = (244, 244, 246)
        else:
            rgb = tuple(int(a[n] + (b[n] - a[n]) * k) for n in range(3))
        out.append((int(x), int(y), "#%02X%02X%02X" % rgb))

    for s in range(p["streams"]):
        sd = s * 19.7 + p["seed"]
        cy = rows * (0.5 if p["streams"] == 1 else 0.22 + 0.56 * s / (p["streams"] - 1))
        thickness = max(rows * p["thick"] * (0.75 + 0.5 * h(s, 7.0, sd)), 0.8)
        for x in range(min(int(reach), int(cols))):
            u = x / reach
            tp = (1.0 - u) ** 0.55
            yc = (cy + math.sin(u * p["wave"] + t + sd) * rows * p["amp"] * (0.3 + u)
                  + math.sin(u * p["wave"] * 2.7 - t * 2.0 + sd * 1.3) * rows * p["amp"] * 0.25)
            th = max(thickness * tp * (0.85 + 0.15 * math.sin(t * 2.0 + u * 11.0 + sd)), 0.55)
            for y in range(math.floor(yc - th), math.ceil(yc + th) + 1):
                if y < 0 or y >= rows:
                    continue
                d = (y + 0.5 - yc) / th
                if not (-1.0 <= d <= 1.0) or h(x, y + s * 101.0, fr + sd) > 0.3 + 0.8 * tp:
                    continue
                v = (1.0 - d * d * 0.6) * (0.45 + 0.55 * tp) * p["int"]
                dither = BAYER[(x & 3) + ((y & 3) << 2)] / 16.0
                level = math.floor(v * 2.0 + dither) / 2.0
                if level <= 0.0:
                    continue
                paint(x, y, (p.get("mono") if p.get("mono") is not None else d * 0.5 + 0.5) + u * 0.12,
                      min(level, 1.0))
        for q in range(math.floor(rows * 0.5 + 4.0)):
            fx = math.floor(reach + h(q, s, fr + 3.0) * cols * 0.12)
            fy = math.floor(cy + (h(q, s + 9.0, fr + 5.0) - 0.5) * rows * p["thick"] * 2.3)
            if fx >= cols or fy < 0 or fy >= rows or h(q, s, 77.0) > 0.45:
                continue
            paint(fx, fy, h(q, s, 31.0), 0.5)
    return out


def draw_prism(cells, ox, oy, width, height, p, frame):
    px = p["px"]
    gap = 1 if px >= 4 else 0
    for x, y, color in prism_cells(width, height, p, frame):
        cells.add(ox + x * px, oy + y * px, px - gap, px - gap, color)


def grain(cells, ox, oy, width, height, amount, frame):
    n = int(width * height * 0.0008 * amount)
    for i in range(n):
        a = 0.06 + 0.22 * h(i, frame, 3.0)
        x = math.floor(h(i, frame, 1.0) * width)
        y = math.floor(h(i, frame, 2.0) * height)
        level = int(round(a * 255))
        g = "#%02X%02X%02X" % (level, level, level)
        cells.add(ox + x, oy + y, 1, 1, g)


STRIP = dict(px=2, streams=1, reach=1.02, thick=0.8, amp=0.04, wave=8.0, seed=1.0, int=0.85)


def pulse(px):
    return dict(px=px, streams=3, reach=0.84, thick=0.13, amp=0.13, wave=5.0, seed=9.0, int=0.9)


# ---------------------------------------------------------------- svg helpers


def text(x, y, s, size=14, fill=INK, weight=400, anchor="start", extra=""):
    return (f'<text x="{fmt(x)}" y="{fmt(y)}" font-family="{MONO}" font-size="{size}" '
            f'font-weight="{weight}" fill="{fill}" text-anchor="{anchor}"{extra}>{escape(s)}</text>')


def svg_doc(w, h, body, title):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" '
            f'role="img" aria-label="{escape(title)}" shape-rendering="crispEdges">\n'
            f"<title>{escape(title)}</title>\n{body}\n</svg>\n")


def card_frame(w, h, cid):
    return (f'<defs><clipPath id="{cid}"><rect width="{w}" height="{h}" rx="16"/></clipPath>'
            f'<pattern id="{cid}-dots" width="12" height="12" patternUnits="userSpaceOnUse">'
            f'<rect x="6" y="6" width="1" height="1" fill="#A0A0BE" fill-opacity="0.28"/></pattern></defs>\n'
            f'<rect width="{w}" height="{h}" rx="16" fill="{BG}"/>')


def card_border(w, h):
    return f'<rect x="0.5" y="0.5" width="{w - 1}" height="{h - 1}" rx="15.5" fill="none" stroke="#1E1E28"/>'


# ---------------------------------------------------------------- banner


def banner():
    W, H = 1280, 470
    body = [card_frame(W, H, "b")]
    body.append('<g clip-path="url(#b)">')
    cells = Cells()

    # top-edge strip, as in the app
    draw_prism(cells, 0, 0, W, 6, STRIP, 2)

    # the flock and the wordmark
    u = 8
    fx, fy = 72, 70
    flock(cells, fx, fy, u)
    wu = 8
    wy = fy + (9 * u - 5 * wu) // 2
    pixel_text(cells, "SHEPHERD", fx + 18 * u + 36, wy, wu, INK)

    # herd pulse along the bottom, with its dot grid
    band_y, band_h = 300, 150
    body.append(f'<rect x="0" y="{band_y}" width="{W}" height="{band_h}" fill="url(#b-dots)"/>')
    draw_prism(cells, 0, band_y, W, band_h, pulse(5), 4)
    grain(cells, 0, band_y, W, band_h, 0.4, 4)
    body.append(cells.svg())

    body.append(text(fx, 214, "The orchestrator for your coding agents.", 38, INK, 700))
    body.append(text(fx, 254, "Teams of Claude, Codex and Gemini agents with roles, working in", 21, MUTED))
    body.append(text(fx, 284, "parallel, each in its own git worktree. You keep every terminal.", 21, MUTED))
    body.append(text(W - 56, 106, "one terminal.", 18, MUTED, 400, "end"))
    body.append(text(W - 56, 132, "the whole flock.", 18, INK, 700, "end"))
    body.append("</g>")
    body.append(card_border(W, H))
    return svg_doc(W, H, "\n".join(body), "Shepherd: the orchestrator for your coding agents")


# ---------------------------------------------------------------- run flow


def run_flow():
    """Drawn at 960 wide so it reads close to 1:1 in a README column."""
    W, H = 960, 324
    body = [card_frame(W, H, "r")]
    body.append('<defs><marker id="r-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="8" '
                f'markerHeight="8" orient="auto"><path d="M0 0L8 4L0 8z" fill="{DIM}"/></marker></defs>')
    body.append('<g clip-path="url(#r)">')
    cells = Cells()
    draw_prism(cells, 0, 0, W, 6, STRIP, 7)

    steps = [
        ("Plan", ["The brain splits", "the task into", "subtasks."], "approve the plan"),
        ("Implement", ["One coder per", "subtask, each in", "its own worktree."], None),
        ("Review", ["A reviewer on", "another vendor,", "up to N rounds."], None),
        ("Checks", ["Your checks, or", "ones detected", "from the language."], None),
        ("Publish", ["Push and a draft", "PR with gh, only", "after you approve."], "approve each push"),
    ]
    x0, col, ny = 32, 184, 44
    for i, (name, lines, decide) in enumerate(steps):
        x = x0 + i * col
        color = ORANGE if decide else "#3A3A48"
        body.append(f'<rect x="{x + 0.5}" y="{ny + 0.5}" width="39" height="39" rx="6" fill="{CARD}" stroke="{color}"/>')
        num = str(i + 1)
        nw = pixel_width(num, 4)
        pixel_text(cells, num, x + 20 - nw / 2, ny + 10, 4, ORANGE if decide else INK)
        if i < len(steps) - 1:
            body.append(f'<path d="M{x + 52} {ny + 20}H{x + col - 12}" stroke="{DIM}" stroke-width="2" '
                        f'stroke-dasharray="2 4" marker-end="url(#r-arrow)"/>')
        body.append(text(x, ny + 74, name, 19, INK, 700))
        for j, ln in enumerate(lines):
            body.append(text(x, ny + 102 + j * 21, ln, 14, MUTED))
        if decide:
            pixel_text(cells, "YOU DECIDE", x, ny + 176, 2, ORANGE)
            body.append(text(x, ny + 204, decide, 14, ORANGE, 700))
        else:
            body.append(text(x, ny + 204, "automatic", 14, DIM))
    body.append(cells.svg())
    body.append(f'<path d="M32 {H - 52}H{W - 32}" stroke="#1E1E28"/>')
    body.append(text(32, H - 22, "shepherd run <task>", 14, INK, 700))
    body.append(text(W - 32, H - 22, "then you merge, with plain git", 14, CYAN, 700, "end"))
    body.append("</g>")
    body.append(card_border(W, H))
    return svg_doc(W, H, "\n".join(body),
                   "How a task run goes: plan, implement, review, checks, publish; you approve the plan and each push")


# ---------------------------------------------------------------- patterns


class Diagram:
    def __init__(self, ox, oy):
        self.ox, self.oy = ox, oy
        self.shapes, self.lines = [], []

    def agent(self, cx, cy, label, w=96, start=False, sub=None):
        x, y = self.ox + cx - w / 2, self.oy + cy - 15
        stroke = INK if start else "#3A3A48"
        fill = "#16161F" if start else "#101017"
        self.shapes.append(f'<rect x="{fmt(x + 0.5)}" y="{fmt(y + 0.5)}" width="{w - 1}" height="29" rx="4" '
                           f'fill="{fill}" stroke="{stroke}"/>')
        self.shapes.append(text(self.ox + cx, self.oy + cy + 4.5, label, 12.5, INK if start else "#D4D4DC",
                                700 if start else 400, "middle"))
        if sub:
            self.shapes.append(text(self.ox + cx, self.oy + cy + 29, sub, 10.5, DIM, 400, "middle"))
        return (cx, cy, w / 2, 15)

    def topic(self, cx, cy, label, w=None):
        w = w or (len(label) * 7.4 + 22)
        x, y = self.ox + cx - w / 2, self.oy + cy - 11
        self.shapes.append(f'<rect x="{fmt(x + 0.5)}" y="{fmt(y + 0.5)}" width="{fmt(w - 1)}" height="21" rx="10.5" '
                           f'fill="{BG}" stroke="{VIOLET}" stroke-opacity="0.85"/>')
        self.shapes.append(text(self.ox + cx, self.oy + cy + 4, label, 12, VIOLET, 400, "middle"))
        return (cx, cy, w / 2, 11)

    def link(self, a, b, both=False, dashed=False):
        """Straight line from the bottom of a to the top of b (or sides when level)."""
        ax, ay, aw, ah = a
        bx, by, bw, bh = b
        if abs(ay - by) < 1:
            x1, y1 = (ax + aw, ay) if bx > ax else (ax - aw, ay)
            x2, y2 = (bx - bw, by) if bx > ax else (bx + bw, by)
        else:
            sign = 1 if by > ay else -1
            x1, y1, x2, y2 = ax, ay + sign * ah, bx, by - sign * bh
            if not both:
                y2 -= sign * 3
        self.raw(f"M{fmt(self.ox + x1)} {fmt(self.oy + y1)}L{fmt(self.ox + x2)} {fmt(self.oy + y2)}",
                 both=both, dashed=dashed)

    def raw(self, d, both=False, dashed=False, end=True):
        dash = ' stroke-dasharray="3 4"' if dashed else ""
        start = ' marker-start="url(#p-arrow-back)"' if both else ""
        endm = ' marker-end="url(#p-arrow)"' if end else ""
        self.lines.append(f'<path d="{d}" fill="none" stroke="#4A4A5A" stroke-width="1.5"{dash}{start}{endm}/>')

    def svg(self):
        return "\n".join(self.lines + self.shapes)


def patterns():
    """Two by two, drawn at 960 wide: text on the left of each panel, the topology on the right."""
    W, H = 960, 726
    body = [card_frame(W, H, "p")]
    body.append('<defs>'
                '<marker id="p-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" '
                'orient="auto"><path d="M0 0L8 4L0 8z" fill="#6A6A78"/></marker>'
                '<marker id="p-arrow-back" viewBox="0 0 8 8" refX="1" refY="4" markerWidth="7" markerHeight="7" '
                'orient="auto"><path d="M8 0L0 4L8 8z" fill="#6A6A78"/></marker></defs>')
    body.append('<g clip-path="url(#p)">')
    cells = Cells()
    draw_prism(cells, 0, 0, W, 6, STRIP, 5)

    pw, ph, gap, mx, my = 448, 310, 16, 24, 30
    info = [
        ("orchestrator-worker", ["One orchestrator", "splits the work, a", "pool of workers", "does it."],
         ["parallel features,", "batch work"]),
        ("hierarchical", ["A director, then", "managers, then", "specialists."], ["big objectives:", "epics, then tasks"]),
        ("blackboard", ["Agents share one", "board of", "hypotheses and", "findings."], ["exploratory", "problems"]),
        ("market", ["Requesters ask,", "solvers bid, a", "market-maker", "decides."], ["competing several", "approaches"]),
    ]
    panels = []
    for i, (name, sub, good) in enumerate(info):
        px = mx + (i % 2) * (pw + gap)
        py = my + (i // 2) * (ph + gap)
        body.append(f'<rect x="{px + 0.5}" y="{py + 0.5}" width="{pw - 1}" height="{ph - 1}" rx="8" '
                    f'fill="{CARD}" stroke="{LINE}"/>')
        pixel_text(cells, f"0{i + 1}", px + 20, py + 20, 2, DIM)
        body.append(text(px + 20, py + 52, name, 17, INK, 700))
        for j, ln in enumerate(sub):
            body.append(text(px + 20, py + 84 + j * 20, ln, 13, MUTED))
        body.append(text(px + 20, py + ph - 62, "good for", 12, DIM))
        for j, ln in enumerate(good):
            body.append(text(px + 20, py + ph - 42 + j * 18, ln, 13, "#D4D4DC"))
        panels.append(Diagram(px + 303 - 145, py - 68))

    dw = 290
    cx = dw / 2
    # orchestrator-worker
    d = panels[0]
    o = d.agent(cx, 112, "orchestrator", 116, start=True)
    t1 = d.topic(cx, 166, "#orders", 96)
    ws = [d.agent(cx + dx, 226, "worker", 72) for dx in (-84, 0, 84)]
    t2 = d.topic(cx, 290, "#results", 96)
    d.link(o, t1)
    for w in ws:
        d.link(t1, w)
        d.link(w, t2)
    d.raw(f"M{fmt(d.ox + cx + 48)} {fmt(d.oy + 290)}H{fmt(d.ox + dw - 12)}V{fmt(d.oy + 112)}"
          f"H{fmt(d.ox + cx + 58 + 3)}")
    d.shapes.append(text(d.ox + cx, d.oy + 336, "workers share one consumer group", 11.5, DIM, 400, "middle"))

    # hierarchical
    d = panels[1]
    r = d.agent(cx, 112, "director", 100, start=True)
    t1 = d.topic(cx, 166, "#root-orders", 118)
    ms = [d.agent(cx + dx, 222, "manager", 88) for dx in (-60, 60)]
    t2 = d.topic(cx, 278, "#team-orders", 118)
    ss = [d.agent(cx + dx, 336, "specialist", 64) for dx in (-102, -34, 34, 102)]
    d.link(r, t1)
    for m in ms:
        d.link(t1, m)
        d.link(m, t2)
    for s_ in ss:
        d.link(t2, s_)
    d.shapes = [x.replace('font-size="12.5"', 'font-size="10.5"') if ">specialist<" in x else x for x in d.shapes]

    # blackboard
    d = panels[2]
    bb = d.topic(cx, 222, "#blackboard", 128)
    a1 = d.agent(cx, 128, "researcher", 108, start=True)
    a2 = d.agent(cx - 70, 316, "analyst", 96)
    a3 = d.agent(cx + 70, 316, "synthesizer", 110)
    d.link(a1, bb, both=True)
    d.link(a2, bb, both=True)
    d.link(a3, bb, both=True)
    d.shapes.append(text(d.ox + cx + 10, d.oy + 180, "hypotheses", 11.5, DIM))
    d.shapes.append(text(d.ox + cx - 116, d.oy + 268, "validation", 11.5, DIM))
    d.shapes.append(text(d.ox + cx + 50, d.oy + 268, "write-up", 11.5, DIM))

    # market
    d = panels[3]
    mc = cx - 14
    rq = d.agent(mc, 104, "requester", 104, start=True)
    ask = d.topic(mc, 152, "#ask", 70)
    sv = [d.agent(mc + dx, 202, "solver", 72) for dx in (-56, 56)]
    bid = d.topic(mc, 252, "#bid", 70)
    mm = d.agent(mc, 300, "market-maker", 120)
    tx = d.topic(mc, 350, "#transaction", 116)
    d.link(rq, ask)
    for s_ in sv:
        d.link(ask, s_)
        d.link(s_, bid)
    d.link(bid, mm)
    d.link(mm, tx)
    d.raw(f"M{fmt(d.ox + mc + 35)} {fmt(d.oy + 152)}H{fmt(d.ox + dw - 16)}V{fmt(d.oy + 300)}"
          f"H{fmt(d.ox + mc + 60 + 3)}")

    for d in panels:
        body.append(d.svg())
    body.append(cells.svg())

    ly = H - 22
    body.append(f'<rect x="{mx + 0.5}" y="{ly - 13.5}" width="30" height="17" rx="3" fill="#101017" stroke="#3A3A48"/>')
    body.append(text(mx + 40, ly, "agent", 13, MUTED))
    body.append(f'<rect x="{mx + 110.5}" y="{ly - 13.5}" width="30" height="17" rx="3" fill="#16161F" stroke="{INK}"/>')
    body.append(text(mx + 150, ly, "starts the run", 13, MUTED))
    body.append(f'<rect x="{mx + 290.5}" y="{ly - 12.5}" width="38" height="15" rx="7.5" fill="{BG}" stroke="{VIOLET}"/>')
    body.append(text(mx + 338, ly, "topic on the event bus", 13, MUTED))
    body.append(text(W - mx, ly, "shepherd pattern list", 13, INK, 700, "end"))
    body.append("</g>")
    body.append(card_border(W, H))
    return svg_doc(W, H, "\n".join(body),
                   "The four built-in patterns: orchestrator-worker, hierarchical, blackboard and market")


for name, fn in [("banner", banner), ("run-flow", run_flow), ("patterns", patterns)]:
    svg = fn()
    (OUT / f"{name}.svg").write_text(svg)
    print(name, len(svg) // 1024, "KB")


# ---------------------------------------------------------------- the flock on its own (footer), dark and light

LIGHT = {
    (0, "W"): "#C4C4CE", (0, "E"): "#FFFFFF", (0, "_"): "#B0B0BC",
    (1, "W"): "#8A8A98", (1, "E"): "#FFFFFF", (1, "_"): "#767684",
    (2, "W"): "#2C2C38", (2, "H"): "#5A5A6A", (2, "E"): "#FFFFFF", (2, "_"): "#4A4A5A",
}


def flock_svg(light):
    u = 6
    cells = Cells()
    for x0, y0, t in HERD:
        for ry, row in enumerate(SHEEP):
            for rx, ch in enumerate(row):
                if ch == ".":
                    continue
                c = (LIGHT.get((t, ch)) or LIGHT[(t, "_")]) if light else tone(t, ch)
                cells.add((x0 + rx) * u, (y0 + ry) * u, u, u, c)
    return svg_doc(18 * u, 9 * u, cells.svg(), "Shepherd logo: the flock")


(OUT / "flock-dark.svg").write_text(flock_svg(False))
(OUT / "flock-light.svg").write_text(flock_svg(True))
print("flock ok")
