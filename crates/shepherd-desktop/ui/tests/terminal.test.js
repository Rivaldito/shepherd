// Run with: node crates/shepherd-desktop/ui/tests/terminal.test.js
const assert = require("node:assert/strict");
const { parseAnsiLines, styleToCss, keyToInput, mergeInputs, color256, fitFontSize } = require("../terminal.js");

// --- ANSI: the exact screen text the server returned for a colored prompt + ls
const screen = "\r\n\u001b[0m\u001b[1m\u001b[38;5;6m/var/tmp\u001b[0m \r\n\u001b[0m\u001b[1m\u001b[38;5;2m❯\u001b[0m echo hola\r\nhola\r\n\u001b[0m\u001b[1m\u001b[38;5;4mdir\u001b[0m\r\n";
const lines = parseAnsiLines(screen);
assert.equal(lines.length, 6);
assert.equal(lines[1][0].text, "/var/tmp");
assert.equal(lines[1][0].style.bold, true);
assert.equal(lines[1][0].style.fg, "#7dcfff"); // 38;5;6 -> cyan
assert.deepEqual(lines[3].map((r) => r.text), ["hola"]);
assert.equal(styleToCss(lines[3][0].style), null); // plain text is unstyled
assert.equal(lines[2][1].text, " echo hola");
assert.equal(lines[2][1].style.fg, undefined); // reset by \e[0m

// attributes, truecolor, 256 cube/grayscale, reverse, resets
const a = parseAnsiLines("\x1b[38;2;10;20;30mrgb\x1b[39m\x1b[4;9munder\x1b[0m\x1b[7mrev\x1b[48;5;196mbg")[0];
assert.equal(a[0].style.fg, "rgb(10,20,30)");
assert.equal(a[1].style.fg, undefined);
assert.match(styleToCss(a[1].style), /underline line-through/);
assert.match(styleToCss(a[2].style), /color:#0f0f14.*background:#d8d8e0|background:#d8d8e0.*color:#0f0f14/s);
assert.equal(color256(196), "rgb(255,0,0)");
assert.equal(color256(244), "rgb(128,128,128)");

// non-SGR sequences are dropped, text around them survives; OSC 8 links keep their label
const b = parseAnsiLines("a\x1b[2Kb\x1b[?25lc\x1b]8;;http://x\x07link\x1b]8;;\x07d")[0];
assert.equal(b.map((r) => r.text).join(""), "abclinkd");
assert.deepEqual(parseAnsiLines("")[0], []);

// --- keys
const ev = (key, o = {}) => ({ key, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...o });
assert.deepEqual(keyToInput(ev("a")), { text: "a" });
assert.deepEqual(keyToInput(ev("A", { shiftKey: true })), { text: "A" });
assert.deepEqual(keyToInput(ev("ñ")), { text: "ñ" });
assert.deepEqual(keyToInput(ev("Enter")), { key: "enter" });
assert.deepEqual(keyToInput(ev(" ")), { key: "space" });
assert.deepEqual(keyToInput(ev("c", { ctrlKey: true })), { key: "ctrl+c" });
assert.deepEqual(keyToInput(ev("b", { altKey: true })), { key: "alt+b" });
assert.deepEqual(keyToInput(ev("ArrowUp")), { key: "up" });
assert.deepEqual(keyToInput(ev("ArrowLeft", { ctrlKey: true })), { key: "ctrl+left" });
assert.deepEqual(keyToInput(ev("Tab", { shiftKey: true })), { key: "shift+tab" });
assert.deepEqual(keyToInput(ev("F5")), { key: "f5" });
assert.deepEqual(keyToInput(ev("Delete")), { text: "\x1b[3~" }); // rejected as a key name, so raw
assert.deepEqual(keyToInput(ev("Home")), { text: "\x1b[H" });
assert.equal(keyToInput(ev("C", { ctrlKey: true, shiftKey: true })), null); // browser copy
assert.equal(keyToInput(ev("v", { ctrlKey: true, shiftKey: true })), null); // browser paste
assert.equal(keyToInput(ev("c", { metaKey: true })), null);
assert.equal(keyToInput(ev("Shift")), null);
assert.equal(keyToInput(ev("Dead")), null);

// --- batching keeps order and merges neighbours
assert.deepEqual(mergeInputs([{ text: "l" }, { text: "s" }, { key: "enter" }, { key: "up" }, { text: "x" }]),
  [{ text: "ls" }, { keys: ["enter", "up"] }, { text: "x" }]);
assert.deepEqual(mergeInputs([]), []);

// --- font fit: limited by width or by height, clamped
assert.equal(fitFontSize(600, 4000, 100, 10), 10); // width bound: 600/(100*.6)
assert.equal(fitFontSize(4000, 240, 10, 10), 18); // height bound is 20, clamped to 18
assert.equal(fitFontSize(10, 10, 200, 100), 7);
console.log("terminal.js: all checks passed");
