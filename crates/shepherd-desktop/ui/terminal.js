// Pure terminal helpers: ANSI screen text -> styled runs, browser key events -> server input.
// The server owns the terminal (PTY, parser, size); this only paints what `pane.read` returns.

const ANSI16 = ["#1b1b22", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5",
  "#414868", "#ff899d", "#b9f27c", "#ffc777", "#8db0ff", "#c7a9ff", "#a4daff", "#ffffff"];
const CUBE = [0, 95, 135, 175, 215, 255];

function color256(n) {
  if (n < 16) return ANSI16[n];
  if (n < 232) { n -= 16; return `rgb(${CUBE[Math.floor(n / 36)]},${CUBE[Math.floor(n / 6) % 6]},${CUBE[n % 6]})`; }
  const g = 8 + (n - 232) * 10;
  return `rgb(${g},${g},${g})`;
}

// Applies one SGR parameter list to a style object.
function applySgr(style, params) {
  const p = params.length ? params : [0];
  for (let i = 0; i < p.length; i++) {
    const n = p[i];
    if (n === 0) Object.keys(style).forEach((k) => delete style[k]);
    else if (n === 1) style.bold = true; else if (n === 2) style.dim = true; else if (n === 3) style.italic = true;
    else if (n === 4) style.underline = true; else if (n === 7) style.reverse = true; else if (n === 9) style.strike = true;
    else if (n === 22) { delete style.bold; delete style.dim; } else if (n === 23) delete style.italic;
    else if (n === 24) delete style.underline; else if (n === 27) delete style.reverse; else if (n === 29) delete style.strike;
    else if (n >= 30 && n <= 37) style.fg = ANSI16[n - 30]; else if (n === 39) delete style.fg;
    else if (n >= 40 && n <= 47) style.bg = ANSI16[n - 40]; else if (n === 49) delete style.bg;
    else if (n >= 90 && n <= 97) style.fg = ANSI16[n - 90 + 8]; else if (n >= 100 && n <= 107) style.bg = ANSI16[n - 100 + 8];
    else if (n === 38 || n === 48) {
      const key = n === 38 ? "fg" : "bg";
      if (p[i + 1] === 5) { style[key] = color256(p[i + 2] ?? 0); i += 2; }
      else if (p[i + 1] === 2) { style[key] = `rgb(${p[i + 2] ?? 0},${p[i + 3] ?? 0},${p[i + 4] ?? 0})`; i += 4; }
    }
  }
}

// text with ANSI escapes -> array of lines; each line is an array of {text, style}.
// Only SGR (colors/attributes) is interpreted; other CSI/OSC sequences are dropped, "\r" is ignored.
function parseAnsiLines(text) {
  const lines = [[]], style = {};
  const re = /\x1b\[([0-?]*)[ -\/]*([@-~])|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[@-Z\\-_]|\r|\n|[^\x1b\r\n]+|\x1b/g;
  let m;
  while ((m = re.exec(text)) !== null) {
    const tok = m[0];
    if (tok === "\n") lines.push([]);
    else if (tok === "\r" || tok === "\x1b") continue;
    else if (m[2] !== undefined) { if (m[2] === "m" && /^[0-9;:]*$/.test(m[1])) applySgr(style, m[1].split(/[;:]/).filter((s) => s !== "").map(Number)); }
    else if (tok[0] === "\x1b") continue;
    else lines[lines.length - 1].push({ text: tok, style: { ...style } });
  }
  return lines;
}

// {fg,bg,bold,...} -> inline CSS (null when the run is unstyled).
function styleToCss(s) {
  let fg = s.fg, bg = s.bg;
  if (s.reverse) { [fg, bg] = [bg ?? "#0f0f14", fg ?? "#d8d8e0"]; }
  const css = [];
  if (fg) css.push(`color:${fg}`);
  if (bg) css.push(`background:${bg}`);
  if (s.bold) css.push("font-weight:700");
  if (s.dim) css.push("opacity:.6");
  if (s.italic) css.push("font-style:italic");
  if (s.underline || s.strike) css.push(`text-decoration:${[s.underline && "underline", s.strike && "line-through"].filter(Boolean).join(" ")}`);
  return css.length ? css.join(";") : null;
}

const NAMED_KEYS = { Enter: "enter", Escape: "esc", Tab: "tab", Backspace: "backspace", ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right", " ": "space" };
// The server rejects these as key names, but passes raw escape sequences sent as text.
const RAW_KEYS = { Delete: "\x1b[3~", Home: "\x1b[H", End: "\x1b[F", PageUp: "\x1b[5~", PageDown: "\x1b[6~", Insert: "\x1b[2~" };

// KeyboardEvent-like -> {text} | {key} | null (null = not ours: let the browser/app handle it).
function keyToInput(ev) {
  const k = ev.key;
  if (ev.metaKey) return null;
  if (ev.ctrlKey && ev.shiftKey && /^[cCvV]$/.test(k)) return null; // copy / paste stay with the browser
  if (RAW_KEYS[k] && !ev.ctrlKey && !ev.altKey) return { text: RAW_KEYS[k] };
  const mods = (ev.ctrlKey ? "ctrl+" : "") + (ev.altKey ? "alt+" : "");
  if (/^F([1-9]|1[0-2])$/.test(k)) return { key: mods + k.toLowerCase() };
  if (k === "Tab" && ev.shiftKey) return { key: mods + "shift+tab" };
  if (NAMED_KEYS[k]) return { key: mods + NAMED_KEYS[k] };
  if (k.length === 1) return ev.ctrlKey || ev.altKey ? { key: mods + k.toLowerCase() } : { text: k };
  return null;
}

// Coalesces a burst of inputs into the fewest requests, keeping order:
// consecutive texts become one `send_text`, consecutive keys one `send_keys`.
function mergeInputs(items) {
  const out = [];
  for (const it of items) {
    const last = out[out.length - 1];
    if (it.text !== undefined) { if (last && last.text !== undefined) last.text += it.text; else out.push({ text: it.text }); }
    else if (last && last.keys) last.keys.push(it.key); else out.push({ keys: [it.key] });
  }
  return out;
}

// Font size that fits `cols` x `rows` cells in a box (JetBrains Mono: ~0.6em wide, line-height 1.2).
function fitFontSize(width, height, cols, rows) {
  const size = Math.min(width / (Math.max(cols, 1) * 0.6), height / (Math.max(rows, 1) * 1.2));
  return Math.max(7, Math.min(18, Math.floor(size * 10) / 10));
}

if (typeof module !== "undefined") module.exports = { parseAnsiLines, styleToCss, keyToInput, mergeInputs, color256, fitFontSize };
