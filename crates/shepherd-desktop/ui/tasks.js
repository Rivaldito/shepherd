// Tasks tab, like the TUI: filter (Todas / Abiertas / Completadas) + list + detail, for the project of
// the active workspace. Tasks can be created here, and agents are launched for a task and listed under it
// (the registry keeps `task_id` on each agent: that is what "assigned" means).
let tk = { list: [], agents: [], sel: null, raw: "", filter: 0, dir: "", creating: false, newForm: null, spawning: null, spawnEl: null }; // sel: task id

const TASK_FILTERS = ["Todas", "Abiertas", "Completadas"];
const isDoneTask = (t) => ["done", "completed", "archived"].includes(t.state.toLowerCase());
const isOpen = (t) => !isDoneTask(t); // "new" tasks are open too
const taskShown = (t) => tk.filter === 1 ? isOpen(t) : tk.filter === 2 ? isDoneTask(t) : true;
const taskBadge = (s) => {
  const k = s.toLowerCase();
  return [`[${s.toUpperCase()}]`, ["open", "active"].includes(k) ? "ok" : ["done", "completed"].includes(k) ? "dim" : k === "blocked" ? "bad" : "warn"];
};

async function fetchTasks() {
  const [list, dir] = await Promise.all([invokeP("task_list"), invokeP("task_dir")]);
  let agents = [];
  if (sb.status?.running) {
    try { agents = (await invoke("api_call", { method: "shepherd.agent.list", params: {} })).agents; } catch (_) { /* layer off or server gone */ }
  }
  return { list, dir, agents };
}

function applyTasks(data) {
  const raw = JSON.stringify([data.list, data.agents, data.dir]);
  if (raw === tk.raw) return;
  tk.raw = raw; tk.list = data.list; tk.agents = data.agents; tk.dir = data.dir;
  renderTasks();
}

async function drawTasks() {
  poll("tasks", refreshTasks); // tasks are files under .shepherd/tasks; the registry is polled with them
  tk.raw = "";
  try { applyTasks(await fetchTasks()); }
  catch (e) { $("list").replaceChildren(el("div", { className: "empty" }, String(e))); $("detail").replaceChildren(); }
}

async function refreshTasks() {
  try { applyTasks(await fetchTasks()); } catch (_) { /* keep last view */ }
}

// "+ Nueva tarea": the same fields the TUI asks for (title, description, criteria one per line).
function newTaskForm() {
  const title = el("input", { type: "text", className: "inline", placeholder: "Título de la tarea" });
  const desc = el("textarea", { className: "inline", rows: 4, placeholder: "Descripción / especificación" });
  const crit = el("textarea", { className: "inline", rows: 3, placeholder: "Criterios de aceptación (uno por línea)" });
  const msg = el("div", { className: "dim" });
  const cancel = () => { tk.creating = false; tk.newForm = null; renderTasks(); };
  const create = async () => {
    if (shepherdOff()) { msg.textContent = "Activa Shepherd primero (aviso bajo los paneles)."; return; }
    if (!title.value.trim()) { msg.textContent = "Escribe un título."; return; }
    const params = { title: title.value.trim() };
    if (desc.value.trim()) params.description = desc.value.trim();
    const criteria = crit.value.split("\n").map((s) => s.trim()).filter(Boolean);
    if (criteria.length) params.criteria = criteria;
    // the project root (where `.shepherd/` is), not the terminal's subfolder
    params.root = await invoke("resolved_root", { root: projectRoot() });
    msg.textContent = "Creando…";
    const r = await act("shepherd.task.create", params);
    if (!r) { msg.textContent = `No se pudo crear: ${ws.lastError || "error desconocido"}`; return; }
    tk.creating = false; tk.newForm = null; tk.sel = r.task.status.id; tk.filter = 0; tk.raw = "";
    flash(`Tarea #${r.task.status.id} creada`); refreshTasks();
  };
  const form = el("div", { className: "aform" }, el("label", {}, "Título"), title, el("label", {}, "Descripción"), desc, el("label", {}, "Criterios"), crit, msg,
    el("div", { className: "row" }, el("button", { className: "btn small", onclick: create }, "Crear tarea"), el("button", { className: "mini", onclick: cancel }, "Cancelar")));
  form.addEventListener("keydown", (e) => { e.stopPropagation(); if (e.key === "Escape") cancel(); });
  setTimeout(() => title.focus(), 0);
  return form;
}

// Launch an agent for this task: the task text is prefilled and the agent is created in the active workspace.
function spawnForTask(t) {
  const close = () => { tk.spawning = null; tk.spawnEl = null; tk.raw = ""; refreshTasks(); };
  tk.spawning = t.id;
  tk.spawnEl = spawnForm({ taskId: t.id, taskText: taskPrompt(t), onDone: (agent) => { close(); if (agent.pane) focusPane(agent.pane); }, onCancel: () => { tk.spawning = null; tk.spawnEl = null; renderTasks(); } });
  renderTasks();
}

function assignedBlock(t) {
  const mine = agentsOfTask(tk.agents, t.id);
  const rows = mine.map((a) => {
    const [txt, cls] = stateBadge(a.state);
    return el("div", { className: "arow" }, el("span", {}, `${a.role} (${a.id}) `, el("span", { className: "badge " + cls }, txt)),
      el("span", { className: "dim" }, a.workspace ? `space ${a.workspace}` : ""), el("span", { className: "grow" }),
      a.pane && !TERMINAL_STATES.includes(a.state) ? el("button", { className: "mini", onclick: () => focusPane(a.pane) }, "Ir al terminal") : "",
      el("button", { className: "mini", title: "Quitar de esta tarea", onclick: async () => { if (await assignAgent(a.id, null)) { tk.raw = ""; refreshTasks(); } } }, "Quitar"));
  });
  const running = !!sb.status?.running;
  // Existing agents: anyone not already on this task (the label says which task they are on now).
  const others = tk.agents.filter((a) => String(a.task_id ?? "") !== String(t.id));
  const pick = el("select", {}, ...(others.length ? others.map((a) => el("option", { value: a.id }, `${a.role} (${a.id})${a.task_id ? ` · ahora en la tarea #${a.task_id}` : ""}`)) : [el("option", { value: "" }, "No hay otros agentes")]));
  const assign = el("div", { className: "assign" }, pick, el("button", { className: "btn small", disabled: !others.length, onclick: async () => {
    if (pick.value && await assignAgent(pick.value, t.id)) { flash(`Agente ${pick.value} asignado a la tarea #${t.id}`); tk.raw = ""; refreshTasks(); }
  } }, "Asignar existente"));
  return el("div", { className: "block" }, el("h4", {}, "AGENTES ASIGNADOS:"),
    ...(rows.length ? rows : [el("div", { className: "dim" }, "Ninguno todavía.")]), assign,
    el("button", { className: "btn small", disabled: !running, title: running ? "" : "Necesita el servidor en marcha", style: "margin-top:8px", onclick: () => spawnForTask(t) }, "+ Lanzar agente para esta tarea"));
}

function renderTasks() {
  const shown = tk.list.filter(taskShown);
  if (!shown.some((t) => t.id === tk.sel)) tk.sel = shown[0]?.id ?? null;
  const filters = el("div", { className: "filters" }, ...TASK_FILTERS.map((f, i) =>
    el("button", { className: i === tk.filter ? "sel" : "", onclick: () => { tk.filter = i; renderTasks(); } }, f)));
  const items = el("div", { id: "items" });
  for (const t of shown) {
    const [txt, cls] = taskBadge(t.state);
    const n = agentsOfTask(tk.agents, t.id).length;
    items.append(el("button", { className: "item" + (t.id === tk.sel && !tk.creating ? " sel" : ""), onclick: () => { tk.sel = t.id; tk.creating = false; renderTasks(); } },
      el("span", {}, `#${String(t.id).padStart(2, "0")} ${t.title} `, el("span", { className: "badge " + cls }, txt)), el("small", {}, n ? `${n} agente(s)` : "")));
  }
  $("list").replaceChildren(filters, items, el("div", { className: "panel-actions" },
    el("button", { className: "btn small", onclick: () => { tk.creating = true; tk.newForm ??= newTaskForm(); renderTasks(); } }, "+ Nueva tarea"),
    el("button", { className: "btn small", onclick: () => { tk.raw = ""; refreshTasks(); } }, "Refrescar")));
  if (tk.creating) return $("detail").replaceChildren(el("div", { className: "head" }, "NUEVA TAREA", el("span", { className: "state" })), el("div", { className: "blocks" }, tk.newForm),
    el("div", { className: "meta" }, `Se guardará en ${tk.dir}`));
  const t = tk.list.find((x) => x.id === tk.sel);
  if (!t) return $("detail").replaceChildren(el("div", { className: "empty" },
    `No hay tareas que coincidan con el filtro en este proyecto (${tk.dir}). Crea una con «+ Nueva tarea».`));

  const [txt, cls] = taskBadge(t.state);
  const block = (title, ...body) => el("div", { className: "block" }, el("h4", {}, title), ...body);
  const text = (s) => el("pre", {}, s);
  const detail = el("div", { className: "blocks" });
  if (tk.spawning === t.id && tk.spawnEl) detail.append(tk.spawnEl); // built once: a refresh must not wipe what is typed
  if (t.waiting) detail.append(el("div", { className: "notice" }, `ESPERANDO: ${t.waiting}`));
  if (t.description) detail.append(block("ESPECIFICACIÓN:", text(t.description)));
  if (t.criteria.length) detail.append(block("CRITERIOS DE ACEPTACIÓN:", text(t.criteria.map((c) => {
    const done = /^- \[[xX]\]/.test(c);
    return `${done ? "☑" : "☐"} ${c.replace(/^([-*] \[[ xX]\]|[-*])\s*/, "")}`;
  }).join("\n"))));
  detail.append(assignedBlock(t));
  if ((t.subtasks ?? []).length) detail.append(block("SUBTAREAS:", text(t.subtasks.map((s) =>
    `${s.n}. ${s.title} [${s.state}]${s.branch ? " " + s.branch : ""}${s.pr ? " PR " + s.pr : ""}${s.note ? "\n     " + s.note : ""}`).join("\n"))));
  if (t.links.length) detail.append(block("ENLACES:", text(t.links.join("\n"))));
  if (t.note) detail.append(block("NOTA:", text(t.note)));
  if (t.plan) detail.append(block("PLAN:", text(t.plan)));
  $("detail").replaceChildren(
    el("div", { className: "head" }, `TAREA #${t.id}: ${t.title}`, el("span", { className: "badge " + cls }, txt), el("span", { className: "state" })),
    el("div", { className: "meta" }, `Estado: ${t.state} | Rama: ${t.branch || "ninguna"} | PR: #${t.pr ?? 0} | Proyecto: ${tk.dir.replace(/\/\.shepherd\/tasks$/, "")}`), detail);
}
