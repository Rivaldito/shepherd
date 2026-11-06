// Run with: node crates/shepherd-desktop/ui/tests/discover.test.js
const assert = require("node:assert/strict");
const { dc, agentNameOk, sanitizeAgentName, uniqueName, mergeModels, newModelIds, kindOfVendor, vendorForKind, modelFlagFor, kindChoices, isInstalledKind } = require("../discover.js");

// --- mergeModels: detected first, then configured, then the agent's default; one entry per id
const merged = mergeModels({
  live: [{ id: "gemini-4-argon-medium", label: "Gemini 4 Argon (Medium)" }, { id: "claude-opus-5-5-high", label: "Claude Opus 5.5 (High)" }],
  configured: ["claude-opus-5-5-high", "opus"],
  agentDefault: "sonnet",
});
assert.deepEqual(merged.map((m) => m.id), ["gemini-4-argon-medium", "claude-opus-5-5-high", "opus", "sonnet"]);
assert.deepEqual(merged.map((m) => m.source), ["detectado", "detectado", "configuración", "predeterminado del agente"]);
assert.equal(merged[0].label, "gemini-4-argon-medium — Gemini 4 Argon (Medium)"); // id first: that is what gets typed
assert.equal(merged[2].label, "opus");
assert.deepEqual(mergeModels({ live: [{ id: "a", label: "a" }], agentDefault: "a" }).map((m) => m.id), ["a"]); // label == id adds nothing
assert.deepEqual(mergeModels({}), []);
assert.deepEqual(mergeModels({ agentDefault: null, configured: ["", "x"] }).map((m) => m.id), ["x"]);

// --- agent names: the rules the server enforces (probed against a real server)
for (const ok of ["pi", "pi-2", "under_score", "dash-ed", "a" + "b".repeat(31)]) assert.equal(agentNameOk(ok), true, ok);
for (const bad of ["", "with space", "UPPER", "ñandú", "dot.name", "a,b", "1start", "-lead", "_lead", "a/b", "a" + "b".repeat(32)]) assert.equal(agentNameOk(bad), false, bad);
assert.equal(sanitizeAgentName("Mi Agénte 2"), "mi-agente-2");
assert.equal(sanitizeAgentName("  --Hello,  World!  "), "hello-world");
assert.equal(sanitizeAgentName("123"), ""); // nothing usable: the caller falls back to a default
assert.equal(sanitizeAgentName("a.b/c"), "a-b-c");
assert.equal(sanitizeAgentName("x".repeat(40)), "x".repeat(32));
assert.equal(sanitizeAgentName("ab-" + "c".repeat(29) + "-d"), "ab-" + "c".repeat(29)); // cut at 32, no trailing dash
for (const raw of ["Mi Agénte 2", "  --Hello,  World!  ", "x".repeat(60), "Ünïcode Näme"]) assert.equal(agentNameOk(sanitizeAgentName(raw)), true, raw);

// --- uniqueName: agent names are unique on the server
assert.equal(uniqueName("pi", new Set()), "pi");
assert.equal(uniqueName("pi", new Set(["pi"])), "pi-2");
assert.equal(uniqueName("pi", new Set(["pi", "pi-2", "pi-4"])), "pi-3"); // first free, not last+1
assert.equal(uniqueName("pi", new Set(["claude"])), "pi");
const long = "k".repeat(32);
assert.equal(agentNameOk(uniqueName(long, new Set([long]))), true); // the suffix still fits in 32
assert.equal(uniqueName(long, new Set([long])), "k".repeat(30) + "-2");

// --- newModelIds: what the "save detected models" button would add
const live = [{ id: "a" }, { id: "b" }, { id: "c" }];
assert.deepEqual(newModelIds(live, ["b"]), ["a", "c"]);
assert.deepEqual(newModelIds(live, ["a", "b", "c"]), []); // nothing new: no button
assert.deepEqual(newModelIds([], ["a"]), []);

// --- which agent a Shepherd vendor runs
assert.equal(kindOfVendor({ shepherd_agent: "agy", command: "agy" }), "agy");
assert.equal(kindOfVendor({ shepherd_agent: "", command: "/usr/local/bin/codex" }), "codex");
assert.equal(kindOfVendor({ command: "claude" }), "claude");
assert.equal(kindOfVendor({ shepherd_agent: "", command: "" }), "");

// --- vendor lookup and model flags (config wins, built-in table is the fallback)
dc.vendors = { claude: { command: "claude", shepherd_agent: "claude", model_flag: "--model" }, gemini: { command: "agy", shepherd_agent: "agy", model_flag: "--m" } };
assert.equal(vendorForKind("agy"), "gemini");
assert.equal(vendorForKind("opencode"), null);
assert.equal(modelFlagFor("agy"), "--m"); // the config's flag beats the built-in one
assert.equal(modelFlagFor("claude"), "--model");
assert.equal(modelFlagFor("codex"), "-m"); // no vendor: built-in table
assert.equal(modelFlagFor("opencode"), null); // unknown: the model is not passed as a flag

// --- installed first, in the server's order; unknown commands are not offered as kinds
dc.detected = [
  { command: "pi", path: null, known: true }, { command: "claude", path: "/b/claude", known: true }, { command: "cat", path: "/bin/cat", known: false },
  { command: "codex", path: "/b/codex", known: true }, { command: "amp", path: null, known: true },
];
assert.deepEqual(kindChoices().map((d) => d.command), ["claude", "codex", "pi", "amp"]);
assert.equal(isInstalledKind("claude"), true);
assert.equal(isInstalledKind("pi"), false);
assert.equal(isInstalledKind("nope"), false);

console.log("discover.js: all checks passed");
