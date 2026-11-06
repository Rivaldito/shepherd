// Run with: node crates/shepherd-desktop/ui/tests/projects.test.js
const assert = require("node:assert/strict");
const { checkoutFor, workspaceRootOf, agentsOfTask, taskPrompt } = require("../projects.js");

// --- checkoutFor: the worktree that contains the terminal's folder, matched on path boundaries
const wts = ["/home/u/app", "/home/u/app-wt/feature", "/home/u/app-wt/fix"];
assert.equal(checkoutFor("/home/u/app", wts), "/home/u/app");
assert.equal(checkoutFor("/home/u/app/src/deep", wts), "/home/u/app");
assert.equal(checkoutFor("/home/u/app-wt/feature/crates", wts), "/home/u/app-wt/feature");
assert.equal(checkoutFor("/home/u/app-wt/feature-x", wts), null); // "feature" is not a prefix of "feature-x" as a folder
assert.equal(checkoutFor("/home/u/application", wts), null); // "/home/u/app" is not a folder prefix of "/home/u/application"
assert.equal(checkoutFor("/home/u/app/nested", ["/home/u/app", "/home/u/app/nested"]), "/home/u/app/nested"); // longest wins
assert.equal(checkoutFor("/elsewhere", wts), null);
assert.equal(checkoutFor("", wts), null);
assert.equal(checkoutFor("/a/b", ["/a/b/"]), "/a/b/"); // a trailing slash still matches

// --- workspaceRootOf
assert.equal(workspaceRootOf({ worktree: { checkout_path: "/r/wt" } }, "/r/wt/src"), "/r/wt");
assert.equal(workspaceRootOf({}, "/r/plain"), "/r/plain");
assert.equal(workspaceRootOf(undefined, "/r/plain"), "/r/plain");
assert.equal(workspaceRootOf({}, ""), null);
assert.equal(workspaceRootOf(null, null), null);

// --- agentsOfTask: the registry keeps task_id as text; ids compare as text; no task means no match
const reg = [{ id: "a-1", task_id: "3" }, { id: "a-2", task_id: "13" }, { id: "a-3" }, { id: "a-4", task_id: "" }, { id: "a-5", task_id: "3" }];
assert.deepEqual(agentsOfTask(reg, 3).map((a) => a.id), ["a-1", "a-5"]);
assert.deepEqual(agentsOfTask(reg, "3").map((a) => a.id), ["a-1", "a-5"]);
assert.deepEqual(agentsOfTask(reg, 1).map((a) => a.id), []); // "13" does not match 1
assert.deepEqual(agentsOfTask([], 3), []);

// --- taskPrompt
assert.equal(taskPrompt({ id: 7, title: "Login", description: "  Añadir login  ", criteria: ["- [ ] funciona", "- [x] con tests", "* otro", ""] }),
  "Tarea #7: Login\n\nAñadir login\n\nCriterios de aceptación:\n- funciona\n- con tests\n- otro");
assert.equal(taskPrompt({ id: 1, title: "Solo título" }), "Tarea #1: Solo título");
assert.equal(taskPrompt({ id: 2, title: "T", description: "", criteria: [] }), "Tarea #2: T");

console.log("projects.js: all checks passed");
