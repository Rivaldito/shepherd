// Every request the UI sends must use parameter names from the server's schema: the server
// silently ignores unknown params (a misnamed `pane_id` on pane.split split the wrong pane).
// Run with: node crates/shepherd-desktop/ui/tests/api-params.test.js
const fs = require("node:fs"), path = require("node:path"), assert = require("node:assert/strict");
const root = path.join(__dirname, "..", "..", "..", "..");
const schema = JSON.parse(fs.readFileSync(path.join(root, "docs/next/api/shepherd-api.schema.json"), "utf8"));
const defs = schema.schemas.request.$defs;

const params = {}; // method -> allowed param names
(function walk(o) {
  if (Array.isArray(o)) return o.forEach(walk);
  if (o && typeof o === "object") {
    const m = o.properties?.method?.const;
    if (m && !(m in params)) params[m] = Object.keys(defs[(o.properties.params?.$ref ?? "").split("/").pop()]?.properties ?? {});
    Object.values(o).forEach(walk);
  }
})(schema.schemas.request);

let checked = 0;
for (const file of fs.readdirSync(path.join(__dirname, "..")).filter((f) => f.endsWith(".js"))) {
  const src = fs.readFileSync(path.join(__dirname, "..", file), "utf8");
  // srv("method", { a: 1, b: x }) and act("method", { ... }); params without braces are skipped
  for (const m of src.matchAll(/\b(?:srv|act)\(\s*"([a-z_.]+)"\s*(?:,\s*\{([^{}]*)\})?/g)) {
    const [, method, body = ""] = m;
    assert.ok(method in params, `${file}: unknown API method ${method}`);
    for (const key of [...body.matchAll(/([A-Za-z_]+)\s*:/g)].map((k) => k[1])) {
      assert.ok(params[method].includes(key), `${file}: ${method} has no parameter "${key}" (allowed: ${params[method].join(", ")})`);
    }
    checked++;
  }
}
assert.ok(checked > 15, `only ${checked} calls checked: the scan is not matching`);
console.log(`api-params: ${checked} calls match the schema`);
