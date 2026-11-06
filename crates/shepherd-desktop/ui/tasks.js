// Tasks tab, like the TUI: filter (Todas / Abiertas / Completadas) + list + detail. Read-only.
let tk = { list: [], sel: null, raw: "", filter: 0, dir: "" }; // sel: task id

const TASK_FILTERS = ["Todas", "Abiertas", "Completadas"];
const isOpen = (t) => ["open", "active"].includes(t.state.toLowerCase());
const isDone = (t) => ["done", "completed", "archived"].includes(t.state.toLowerCase());
const taskShown = (t) => tk.filter === 1 ? isOpen(t) : tk.filter === 2 ? isDone(t) : true;
const taskBadge = (s) => {
  const k = s.toLowerCase();
  return [`[${s.toUpperCase()}]`, ["open", "active"].includes(k) ? "ok" : ["done", "completed"].includes(k) ? "dim" : k === "blocked" ? "bad" : "warn"];
};

function applyTasks(raw) {
  tk.raw = raw; tk.list = JSON.parse(raw);
  renderTasks();
}

async function drawTasks() {
  poll("tasks", refreshTasks); // tasks are files under .shepherd/tasks
  try { tk.dir = await invoke("task_dir"); applyTasks(JSON.stringify(await invoke("task_list"))); }
  catch (e) { $("list").replaceChildren(el("div", { className: "empty" }, String(e))); $("detail").replaceChildren(); }
}

async function refreshTasks() {
  try { const raw = JSON.stringify(await invoke("task_list")); if (raw !== tk.raw) applyTasks(raw); } catch (_) { /* keep last view */ }
}

function renderTasks() {
  const shown = tk.list.filter(taskShown);
  if (!shown.some((t) => t.id === tk.sel)) tk.sel = shown[0]?.id ?? null;
  const filters = el("div", { className: "filters" }, ...TASK_FILTERS.map((f, i) =>
    el("button", { className: i === tk.filter ? "sel" : "", onclick: () => { tk.filter = i; renderTasks(); } }, f)));
  const items = el("div", { id: "items" });
  for (const t of shown) {
    const [txt, cls] = taskBadge(t.state);
    items.append(el("button", { className: "item" + (t.id === tk.sel ? " sel" : ""), onclick: () => { tk.sel = t.id; renderTasks(); } },
      el("span", {}, `#${String(t.id).padStart(2, "0")} ${t.title} `, el("span", { className: "badge " + cls }, txt))));
  }
  $("list").replaceChildren(filters, items, el("button", { id: "new", onclick: refreshTasks }, "Refrescar"));
  const t = tk.list.find((x) => x.id === tk.sel);
  if (!t) return $("detail").replaceChildren(el("div", { className: "empty" },
    `No hay tareas que coincidan con el filtro. Se leen de ${tk.dir}; se crean desde la TUI (pestaña Tareas, tecla n).`));

  const [txt, cls] = taskBadge(t.state);
  const block = (title, ...body) => el("div", { className: "block" }, el("h4", {}, title), ...body);
  const text = (s) => el("pre", {}, s);
  const detail = el("div", { className: "blocks" });
  if (t.waiting) detail.append(el("div", { className: "notice" }, `ESPERANDO: ${t.waiting}`));
  if (t.description) detail.append(block("ESPECIFICACIÓN:", text(t.description)));
  if (t.criteria.length) detail.append(block("CRITERIOS DE ACEPTACIÓN:", text(t.criteria.map((c) => {
    const done = /^- \[[xX]\]/.test(c);
    return `${done ? "☑" : "☐"} ${c.replace(/^([-*] \[[ xX]\]|[-*])\s*/, "")}`;
  }).join("\n"))));
  if ((t.subtasks ?? []).length) detail.append(block("SUBTAREAS:", text(t.subtasks.map((s) =>
    `${s.n}. ${s.title} [${s.state}]${s.branch ? " " + s.branch : ""}${s.pr ? " PR " + s.pr : ""}${s.note ? "\n     " + s.note : ""}`).join("\n"))));
  if ((t.agents ?? []).length) detail.append(block("AGENTES:", text(t.agents.join(", "))));
  if (t.links.length) detail.append(block("ENLACES:", text(t.links.join("\n"))));
  if (t.note) detail.append(block("NOTA:", text(t.note)));
  if (t.plan) detail.append(block("PLAN:", text(t.plan)));
  $("detail").replaceChildren(
    el("div", { className: "head" }, `TAREA #${t.id}: ${t.title}`, el("span", { className: "badge " + cls }, txt), el("span", { className: "state" })),
    el("div", { className: "meta" }, `Estado: ${t.state} | Rama: ${t.branch || "ninguna"} | PR: #${t.pr ?? 0}`), detail);
}
