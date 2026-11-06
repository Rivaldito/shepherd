// Pure helpers about the project a workspace belongs to, and how agents relate to tasks.
// An agent lands in the workspace that has its `root` checkout open (that is how the server places
// it), so "per workspace" means: use that workspace's checkout as the agent's root.

// The checkout (a git worktree path) that contains `cwd`: the longest match, on path boundaries.
function checkoutFor(cwd, paths) {
  if (!cwd) return null;
  const trim = (p) => (p.length > 1 ? p.replace(/\/+$/, "") : p);
  const here = trim(cwd);
  const inside = paths.filter((p) => p && (here === trim(p) || here.startsWith(trim(p) + "/")));
  return inside.sort((a, b) => trim(b).length - trim(a).length)[0] ?? null;
}

// The folder a workspace is about: its linked checkout, else where its terminal is. null = unknown.
function workspaceRootOf(workspace, cwd) {
  return workspace?.worktree?.checkout_path || cwd || null;
}

// Agents of the Shepherd registry that were launched for a task (the registry stores task_id as text).
function agentsOfTask(registry, taskId) {
  return registry.filter((a) => a.task_id !== undefined && a.task_id !== "" && String(a.task_id) === String(taskId));
}

// What an agent is told when launched for a task: the task itself, so nobody retypes it.
function taskPrompt(task) {
  const parts = [`Tarea #${task.id}: ${task.title}`];
  if (task.description) parts.push(task.description.trim());
  const criteria = (task.criteria ?? []).map((c) => c.replace(/^([-*] \[[ xX]\]|[-*])\s*/, "").trim()).filter(Boolean);
  if (criteria.length) parts.push("Criterios de aceptación:\n" + criteria.map((c) => `- ${c}`).join("\n"));
  return parts.join("\n\n");
}

if (typeof module !== "undefined") module.exports = { checkoutFor, workspaceRootOf, agentsOfTask, taskPrompt };
