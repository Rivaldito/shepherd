// startPatternRun: a pattern needing more agents than the limit is confirmed once and reported, any other error is not retried.
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

function load(srv) {
  const calls = [], ctx = { ws: {}, flash() {}, scheduleRefresh() {}, srv: async (m, p) => { calls.push(p); return srv(p, calls.length); } };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.join(__dirname, "..", "launcher.js"), "utf8") + "\nthis.startPatternRun = startPatternRun;", ctx);
  return { start: ctx.startPatternRun, calls, ctx };
}

(async () => {
  const limit = new Error("shepherd_error: confirmation_required: max_concurrent_agents (4) reached (0 active + 7 new); pass confirm_over_limit to proceed");
  let t = load((p, n) => { if (!p.confirm_over_limit) throw limit; return { run: { id: "run-1" } }; });
  let r = await t.start({ pattern: "big", task: "x", root: "/r" });
  assert.equal(r.overLimit, true);
  assert.equal(t.calls.length, 2);
  assert.equal(t.calls[1].confirm_over_limit, true);
  assert.equal(t.calls[1].pattern, "big");

  t = load(() => ({ run: { id: "run-2" } }));
  r = await t.start({ pattern: "small" });
  assert.equal(r.overLimit, false);
  assert.equal(t.calls.length, 1);
  assert.ok(!("confirm_over_limit" in t.calls[0]));

  t = load(() => { throw new Error("pattern_not_found"); });
  r = await t.start({ pattern: "nope" });
  assert.equal(r, null);
  assert.equal(t.calls.length, 1, "other errors are not retried");
  assert.match(t.ctx.ws.lastError, /pattern_not_found/);
  console.log("launcher: ok");
})();
