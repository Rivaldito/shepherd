// Agent actions over the server API: launch (role + task), kill, clean, resume, prompt.
// They change real state, so killing/cleaning/resuming need two clicks (no pop-ups).

const TERMINAL_STATES = ["done", "exited", "killed", "failed"];

// Launch form. `root` is the project folder the agent works in (defaults to the active pane's cwd).
// Used inline in the sidebar and in the Agents tab; `onDone(agent)` runs after a successful launch.
function spawnForm({ root = "", onDone, onCancel }) {
  const role = el("select", { className: "inline" }, el("option", { value: "" }, "cargando roles…"));
  const task = el("textarea", { className: "inline", rows: 3, placeholder: "Tarea del agente" });
  const dir = el("input", { type: "text", className: "inline", placeholder: "carpeta del proyecto", value: root });
  const vendor = el("input", { type: "text", className: "inline", placeholder: "vendor (opcional)" });
  const model = el("input", { type: "text", className: "inline", placeholder: "modelo (opcional)" });
  const worktree = el("input", { type: "checkbox" });
  const msg = el("div", { className: "dim" });
  const submit = el("button", { className: "btn small", onclick: launch }, "Lanzar");

  invoke("get_models").then((m) => {
    const names = Object.keys(m.roles);
    role.replaceChildren(...(names.length ? names.map((n) => el("option", { value: n }, n)) : [el("option", { value: "" }, "sin roles")]));
    if (!names.length) msg.textContent = "No hay roles configurados: créalos en la pestaña Modelos del panel Shepherd.";
  }).catch(() => { role.replaceChildren(el("option", { value: "" }, "sin roles")); });

  async function launch() {
    if (!role.value || !task.value.trim()) { msg.textContent = "Elige un rol y escribe la tarea."; return; }
    const params = { role: role.value, task: task.value.trim(), worktree: worktree.checked };
    if (dir.value.trim()) params.root = dir.value.trim();
    if (vendor.value.trim()) params.vendor = vendor.value.trim();
    if (model.value.trim()) params.model = model.value.trim();
    submit.disabled = true; msg.textContent = "Lanzando…";
    const r = await act("shepherd.agent.spawn", params);
    submit.disabled = false;
    if (!r) { msg.textContent = "No se pudo lanzar (ver la barra inferior)."; return; }
    flash(`Agente ${r.agent.id} lanzado`);
    onDone?.(r.agent);
  }

  const form = el("div", { className: "aform" }, el("label", {}, "Rol"), role, el("label", {}, "Tarea"), task, el("label", {}, "Carpeta"), dir,
    el("label", {}, "Vendor / modelo"), el("div", { className: "row" }, vendor, model),
    el("label", { className: "check" }, worktree, " en un worktree propio"), msg,
    el("div", { className: "row" }, submit, el("button", { className: "mini", onclick: () => onCancel?.() }, "Cancelar")));
  form.addEventListener("keydown", (e) => { e.stopPropagation(); if (e.key === "Escape") onCancel?.(); });
  return form;
}

// Rows for the agents list: what the server detected in panes plus Shepherd registry agents
// whose pane shows no recognised program. Used by the sidebar.
const STATE_GROUP = { starting: "working", working: "working", running: "working", blocked: "blocked", "needs-human": "blocked", done: "done", completed: "done" };
const AGENT_STATUSES = ["blocked", "working", "done", "idle"];

function agentRows(snap, registry) {
  const rows = [], seen = new Set();
  for (const a of snap.agents) {
    if (!a.agent && !a.shepherd) continue;
    seen.add(a.pane_id);
    rows.push({
      status: AGENT_STATUSES.includes(a.agent_status) ? a.agent_status : "idle", pane: a.pane_id, sid: a.shepherd?.id, sstate: a.shepherd?.state,
      title: a.shepherd ? `${a.shepherd.role} (${a.shepherd.id})` : a.name || a.display_agent || a.agent,
      sub: a.shepherd?.notice || baseName(a.cwd) || a.terminal_title_stripped || "", canPrompt: !!a.agent,
    });
  }
  for (const r of registry) {
    if (r.pane && seen.has(r.pane)) continue;
    rows.push({ status: STATE_GROUP[r.state] ?? "idle", pane: r.pane || null, sid: r.id, sstate: r.state, title: `${r.role} (${r.id})`, sub: r.error || r.task_text || "", canPrompt: false });
  }
  return rows;
}

const isFinished = (row) => !!row.sid && TERMINAL_STATES.includes(row.sstate);
