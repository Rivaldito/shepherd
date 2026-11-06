// Agents tab, laid out like the TUI: list (role (id), state badge, vendor / NEEDS YOU) + detail.
// Live from the server's registry when it runs (so actions and state agree), else the saved file.
// Actions: launch, go to terminal, kill / clean (two clicks), resume.
let ag = { list: [], tasks: [], sel: null, raw: "", live: false, creating: false, form: null, confirm: null }; // sel: agent id

const needsYou = (a) => ["blocked", "needs-human"].includes(a.state) || !!(a.notice ?? "").trim();
const stateBadge = (s) => {
  if (["working", "running", "starting"].includes(s)) return ["[WORKING]", "ok"];
  if (["blocked", "needs-human"].includes(s)) return ["[BLOCKED]", "warn"];
  if (["done", "completed"].includes(s)) return ["[DONE]", "dim"];
  if (s === "failed") return ["[FAILED]", "bad"];
  if (s === "killed") return ["[KILLED]", "bad"];
  if (s === "exited") return ["[EXITED]", "dim"];
  return ["[IDLE]", "dim"];
};

async function fetchAgents() {
  const tasks = await invokeP("task_list").catch(() => []); // the project's tasks, to assign agents to
  if (sb.status?.running) {
    try { return { live: true, tasks, list: (await invoke("api_call", { method: "shepherd.agent.list", params: {} })).agents }; }
    catch (_) { /* orchestration layer off or server gone: fall back to the file */ }
  }
  return { live: false, tasks, list: await invoke("agent_list") };
}

function applyAgents({ live, list, tasks }) {
  const raw = JSON.stringify([live, list, tasks]);
  if (raw === ag.raw) return;
  ag.raw = raw; ag.live = live; ag.list = list; ag.tasks = tasks;
  if (!ag.list.some((a) => a.id === ag.sel)) ag.sel = ag.list[0]?.id ?? null;
  renderAgents();
}

async function drawAgents() {
  poll("agents", refreshAgents);
  ag.raw = "";
  try { applyAgents(await fetchAgents()); }
  catch (e) { $("list").replaceChildren(el("div", { className: "empty" }, String(e))); }
}

async function refreshAgents() {
  try { applyAgents(await fetchAgents()); } catch (_) { /* keep the last good view */ }
}

// Runs an action, then re-reads the list; destructive ones were armed with a first click.
async function agentAction(method, params) {
  ag.confirm = null;
  await act(method, params);
  ag.raw = ""; refreshAgents();
}

function armedButton(key, label, armedLabel, run, extra = "") {
  const armed = ag.confirm === key;
  return el("button", { className: "btn small" + (armed ? " danger" : "") + extra, onclick: () => {
    if (armed) return run();
    ag.confirm = key; renderAgents(); setTimeout(() => { if (ag.confirm === key) { ag.confirm = null; renderAgents(); } }, 4000);
  } }, armed ? armedLabel : label);
}

function listFooter() {
  const live = ag.live;
  const buttons = [el("button", { className: "btn small", disabled: !live, title: live ? "" : "Necesita el servidor en marcha", onclick: () => {
    ag.creating = true; ag.form ??= spawnForm({ root: typeof activeCwd === "function" ? activeCwd() : "", onDone: (agent) => { ag.creating = false; ag.form = null; ag.sel = agent.id ?? ag.sel; ag.raw = ""; refreshAgents(); }, onCancel: () => { ag.creating = false; ag.form = null; renderAgents(); } });
    renderAgents();
  } }, "+ Nuevo")];
  if (live) buttons.push(armedButton("resume", "Reanudar", "¿Reanudar todos?", () => agentAction("shepherd.agent.resume", {})));
  buttons.push(el("button", { className: "btn small", onclick: () => { ag.raw = ""; refreshAgents(); } }, "Refrescar"));
  return el("div", { className: "panel-actions" }, ...buttons, el("span", { className: "dim" }, live ? "en vivo" : "archivo guardado"));
}

// "Tarea": which task of the project this agent works for, with a way to change or clear it.
function taskAssignBlock(a) {
  const current = ag.tasks.find((t) => String(t.id) === String(a.task_id ?? ""));
  const pick = el("select", {}, el("option", { value: "" }, "(ninguna)"),
    ...ag.tasks.map((t) => el("option", { value: String(t.id), selected: current?.id === t.id }, `#${t.id} ${t.title}`)));
  const save = el("button", { className: "btn small", disabled: !ag.tasks.length, onclick: async () => {
    if (await assignAgent(a.id, pick.value ? Number(pick.value) : null)) { flash(pick.value ? `Agente ${a.id} asignado a la tarea #${pick.value}` : `Agente ${a.id} sin tarea`); ag.raw = ""; refreshAgents(); }
  } }, "Guardar");
  return el("div", { className: "block" }, el("h4", {}, "TAREA DEL PROYECTO:"),
    el("div", { className: "dim" }, current ? `Asignado a #${current.id} ${current.title}` : a.task_id ? `Asignado a la tarea #${a.task_id}, que no está en este proyecto` : "Sin tarea asignada"),
    el("div", { className: "assign" }, pick, save), ag.tasks.length ? "" : el("div", { className: "dim small" }, "Este proyecto no tiene tareas: crea una en la pestaña Tareas."));
}

function renderAgents() {
  const items = el("div", { id: "items" });
  for (const a of ag.list) {
    const [txt, cls] = stateBadge(a.state);
    items.append(el("button", { className: "item" + (a.id === ag.sel && !ag.creating ? " sel" : ""), onclick: () => { ag.sel = a.id; ag.creating = false; renderAgents(); } },
      el("span", {}, `${a.role} (${a.id}) `, el("span", { className: "badge " + cls }, txt)),
      el("small", {}, needsYou(a) ? "⚠ NEEDS YOU" : a.vendor)));
  }
  $("list").replaceChildren(items, listFooter());
  if (ag.creating) return $("detail").replaceChildren(el("div", { className: "head" }, "LANZAR AGENTE", el("span", { className: "state" })), el("div", { className: "blocks" }, ag.form));
  const a = ag.list.find((x) => x.id === ag.sel);
  if (!a) return $("detail").replaceChildren(el("div", { className: "empty" },
    "No hay agentes en el registro. Lánzalos con «+ Nuevo» (hace falta el servidor en marcha)."));

  const [txt, cls] = stateBadge(a.state);
  const block = (title, body) => el("div", { className: "block" }, el("h4", {}, title), el("pre", {}, body));
  const detail = el("div", { className: "blocks" });
  if (needsYou(a)) detail.append(el("div", { className: "notice" }, "AVISO DESTACADO (NEEDS YOU):\nEsperando al humano: " +
    ((a.notice ?? "").trim() || "Acción requerida en el terminal nativo (diálogo, login o confirmación).")));
  detail.append(block("TAREA ASIGNADA:", a.task_text || "Sin descripción de tarea asignada."));
  detail.append(taskAssignBlock(a));
  const info = [["Worktree", a.worktree], ["Branch", a.branch], ["Padre", a.parent_id], ["Raíz", a.root], ["Tarea", a.task_id],
    ["Skills activas", (a.active_skills ?? []).join(", ")], ["Iniciado", a.started], ["Actualizado", a.updated]].filter(([, v]) => v);
  if (info.length) detail.append(block("DETALLES:", info.map(([k, v]) => `${k}: ${v}`).join("\n")));
  if (a.result) detail.append(block("RESULTADO:", a.result));
  if (a.error) detail.append(block("ERROR:", a.error));
  detail.append(block("SNAPSHOTS:", a.snapshots ? `${a.snapshots} snapshot(s); último: ${a.last_snapshot}.` : "Aún no hay snapshots."));

  const finished = TERMINAL_STATES.includes(a.state), actions = [];
  if (ag.live && a.pane) actions.push(el("button", { className: "btn small", onclick: () => focusPane(a.pane) }, "Ir al terminal"));
  if (ag.live) actions.push(finished
    ? armedButton("clean:" + a.id, "Limpiar", "¿Limpiar?", () => agentAction("shepherd.agent.clean", { ids: [a.id] }))
    : armedButton("kill:" + a.id, "Matar", "¿Matar agente?", () => agentAction("shepherd.agent.kill", { id: a.id })));
  $("detail").replaceChildren(
    el("div", { className: "head" }, `AGENTE: ${a.role} (ID: ${a.id})`, el("span", { className: "badge " + cls }, txt), el("span", { className: "state" }), ...actions),
    el("div", { className: "meta" }, `Vendor: ${a.vendor} | Modelo: ${a.model || "default"} | Terminal Pane: ${a.pane || "ninguno"}`),
    detail);
}
