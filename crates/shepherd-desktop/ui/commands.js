// Command dock: a panel docked in the window (not a pop-up) to search commands, run the shepherd CLI
// ("> workspace list") and call API methods ("@pane.list"). Every command here is also reachable from
// the visible UI; Ctrl+Shift+P and the status-bar button only open the dock.
let cm = { open: false, sel: 0, items: [], ask: null, armed: null, history: [], h: -1, out: [] };

const dockInput = () => $("cmd-input");
const focusedPaneId = () => currentLayout()?.focused_pane_id ?? null;
const spaceName = (w) => w.label || `Workspace ${w.number}`;

function openPanelTab(key) {
  ws.panel = true; renderWorkspace();
  if (leave("tab:" + key, msgBox())) { tab = key; sel = null; pe = null; draw(); }
}

async function newTabHere() {
  const r = await act("tab.create", { workspace_id: ws.wid, focus: true });
  if (r) ws.tid = r.tab.tab_id;
}

// The commands available right now. Dynamic entries (spaces, tabs, agents) come from the snapshot.
function buildCommands() {
  const s = ws.snap, list = [];
  const add = (group, title, run, o = {}) => list.push({ group, title, run, ...o });
  add("Servidor", "Iniciar servidor", startServer);
  add("Servidor", "Refrescar estado", () => { scheduleRefresh(); refreshStatus(); });
  for (const [key, label] of TABS) add("Panel Shepherd", `Abrir ${label}`, () => openPanelTab(key));
  if (!s) return list;
  const pane = focusedPaneId();
  add("Spaces", "Nuevo space…", () => askFor("Nombre del space (Enter = sin nombre)", async (name) => {
    const params = { focus: true }; if (name) params.label = name; if (activeCwd()) params.cwd = activeCwd();
    const r = await act("workspace.create", params);
    if (r) { ws.wid = r.workspace.workspace_id; ws.tid = r.tab.tab_id; ws.panel = false; renderWorkspace(); }
  }));
  if (ws.wid) {
    add("Spaces", "Renombrar space activo…", () => askFor("Nuevo nombre del space", (label) => label && act("workspace.rename", { workspace_id: ws.wid, label })));
    add("Spaces", "Cerrar space activo", () => act("workspace.close", { workspace_id: ws.wid }), { danger: true });
    add("Pestañas", "Nueva pestaña", newTabHere);
  }
  if (ws.tid) {
    add("Pestañas", "Renombrar pestaña activa…", () => askFor("Nuevo nombre de la pestaña", (label) => label && act("tab.rename", { tab_id: ws.tid, label })));
    add("Pestañas", "Cerrar pestaña activa", () => act("tab.close", { tab_id: ws.tid }), { danger: true });
  }
  if (pane) {
    add("Paneles", "Dividir a la derecha", () => act("pane.split", { target_pane_id: pane, direction: "right", focus: true }));
    add("Paneles", "Dividir hacia abajo", () => act("pane.split", { target_pane_id: pane, direction: "down", focus: true }));
    add("Paneles", "Zoom del panel", () => act("pane.zoom", { pane_id: pane }));
    add("Paneles", "Cerrar panel enfocado", () => act("pane.close", { pane_id: pane }), { danger: true });
    for (const [dir, title] of [["left", "Enfocar el panel de la izquierda"], ["right", "Enfocar el panel de la derecha"], ["up", "Enfocar el panel de arriba"], ["down", "Enfocar el panel de abajo"]])
      add("Paneles", title, () => act("pane.focus_direction", { direction: dir }));
    add("Worktrees", "Nuevo worktree…", () => askFor("Rama del worktree", (branch) => branch && act("worktree.create", { branch, workspace_id: ws.wid, focus: true })));
  }
  add("Agentes", "Lanzar agente…", () => { ws.panel = false; ws.forms.agent ??= openSpawnForm(); renderWorkspace(); });
  add("Agentes", "Reanudar agentes", () => act("shepherd.agent.resume", {}), { danger: true });
  for (const d of dc.detected) if (d.known && d.path) add("Agentes", `Iniciar agente: ${d.command}`, () => quickStartAgent({ kind: d.command }));
  add("Agentes", "Detectar agentes instalados de nuevo", async () => { await loadDiscovery(true); flash("Agentes detectados: " + dc.detected.filter((d) => d.known && d.path).map((d) => d.command).join(", ")); });
  add("Tareas", "Nueva tarea…", () => askFor("Título de la tarea", (title) => title && act("shepherd.task.create", { title })));
  add("Patrones", "Ejecutar patrón…", () => askFor("Nombre del patrón", (pattern) => pattern && askFor("Objetivo del run", (task) => act("shepherd.pattern.run.start", task ? { pattern, task } : { pattern }))));
  for (const w of s.workspaces) add("Navegar", `Ir a space: ${spaceName(w)}`, () => selectWorkspace(w.workspace_id));
  for (const t of ws.wid ? tabsOf(ws.wid) : []) add("Navegar", `Ir a pestaña: ${t.label}`, () => selectTab(t.tab_id));
  for (const r of agentRows(s, ws.sagents)) if (r.pane) add("Navegar", `Ir al agente: ${r.title}`, () => focusPane(r.pane));
  return list;
}

// ---- dock state ------------------------------------------------------------------------------
const dockMode = () => (cm.ask ? "ask" : dockInput().value.startsWith(">") ? "cli" : dockInput().value.startsWith("@") ? "api" : "cmd");

function echo(cmd, text, bad = false) {
  cm.out.push({ cmd, text, bad }); cm.out = cm.out.slice(-40);
  renderDock();
}

function askFor(label, cb) { cm.ask = { label, cb }; dockInput().value = ""; renderDock(); dockInput().focus(); }

function runCommand(c) {
  if (c.danger && cm.armed !== c.title) { cm.armed = c.title; echo(c.title, "Es una acción destructiva: pulsa Enter otra vez para confirmar.", true); return; }
  cm.armed = null;
  c.run();
  if (!cm.ask) closeDock();
}

async function runCli(line) {
  const { args, error } = splitArgs(line);
  if (error) return echo("shepherd " + line, error, true);
  if (!args.length) return;
  try {
    const r = await invoke("cli_run", { args });
    const text = [r.stdout.trimEnd(), r.stderr.trim() && `[stderr]\n${r.stderr.trimEnd()}`, r.timed_out ? "[tiempo agotado]" : `[código ${r.code ?? "?"}]`].filter(Boolean).join("\n");
    echo("shepherd " + line, text, r.code !== 0 || r.timed_out);
  } catch (e) { echo("shepherd " + line, String(e), true); }
  scheduleRefresh();
}

async function runApi(line) {
  const parsed = parseApiLine(line);
  if (parsed.error) return echo("@" + line, parsed.error, true);
  try {
    const result = await invoke("api_call", { method: parsed.method, params: parsed.params });
    const text = JSON.stringify(result, null, 2);
    echo(`@${parsed.method} ${JSON.stringify(parsed.params)}`, text.length > 20000 ? text.slice(0, 20000) + "\n… (recortado)" : text);
  } catch (e) { echo(`@${parsed.method}`, String(e), true); }
  scheduleRefresh();
}

function renderDock() {
  const input = dockInput(), v = input.value, mode = dockMode();
  input.placeholder = cm.ask ? cm.ask.label : "Buscar comando…   > CLI   @ API";
  $("cmd-hint").textContent = mode === "ask" ? "Enter acepta · Esc cancela"
    : mode === "cli" ? "CLI: Enter ejecuta «shepherd " + v.slice(1).trim() + "» · ↑↓ historial"
    : mode === "api" ? "API: @método {json}, p. ej. @pane.list · ↑↓ historial"
    : "↑↓ elige · Enter ejecuta · > comando de la CLI · @método {json} para la API";
  const list = $("cmd-list");
  if (mode === "cmd") {
    cm.items = matchCommands(v, buildCommands());
    cm.sel = Math.max(0, Math.min(cm.sel, cm.items.length - 1));
    list.replaceChildren(...(cm.items.length ? cm.items.map((c, i) => el("div", { className: "cmd" + (i === cm.sel ? " sel" : "") + (c.danger ? " danger" : ""), onclick: () => { cm.sel = i; runCommand(c); } },
      el("span", { className: "cg" }, c.group), el("span", {}, c.title), c.danger ? el("small", {}, "pide confirmar") : "")) : [el("div", { className: "dim pad" }, "Ningún comando coincide.")]));
    list.querySelector(".cmd.sel")?.scrollIntoView({ block: "nearest" });
  } else list.replaceChildren(el("div", { className: "dim pad" }, cm.ask ? cm.ask.label : mode === "cli" ? "Comandos permitidos: workspace, tab, pane, agent, worktree, notification, api, status." : "Cualquier método de runtime: workspace.*, tab.*, pane.*, agent.*, worktree.*, layout.*, shepherd.*"));
  const out = $("cmd-out");
  out.replaceChildren(...cm.out.map((e) => el("div", { className: "oe" }, el("div", { className: "dim" }, "$ " + e.cmd), el("div", { className: e.bad ? "bad" : "" }, e.text))));
  out.scrollTop = out.scrollHeight;
}

function openDock() {
  if (!dc.loaded) loadDiscovery().then(() => cm.open && renderDock()); // so "Iniciar agente: …" can list what is installed
  cm.open = true; $("cmdbar").hidden = false; dockInput().value = ""; cm.sel = 0; cm.armed = null; cm.ask = null;
  renderDock(); dockInput().focus();
}

function closeDock() {
  cm.open = false; cm.ask = null; cm.armed = null; $("cmdbar").hidden = true;
  paneEls.get(focusedPaneId())?.pre.focus({ preventScroll: true });
}

function toggleDock() { if (cm.open && document.activeElement === dockInput()) closeDock(); else openDock(); }

function submitDock() {
  const input = dockInput(), mode = dockMode(), v = input.value;
  if (mode === "ask") { const { cb } = cm.ask; cm.ask = null; input.value = ""; cb(v.trim()); if (!cm.ask) closeDock(); else renderDock(); return; }
  if (mode === "cli" || mode === "api") {
    const line = v.slice(1).trim(); if (!line) return;
    if (cm.history[cm.history.length - 1] !== v) cm.history.push(v);
    cm.h = -1; input.value = v[0];
    return mode === "cli" ? runCli(line) : runApi(line);
  }
  const c = cm.items[cm.sel]; if (c) runCommand(c);
}

function initCommands() {
  const input = dockInput();
  input.addEventListener("input", () => { cm.sel = 0; cm.armed = null; cm.h = -1; renderDock(); });
  input.addEventListener("keydown", (e) => {
    e.stopPropagation(); // the dock owns the keyboard while it has focus
    const mode = dockMode();
    if (e.key === "Escape") { e.preventDefault(); if (cm.ask) { cm.ask = null; input.value = ""; renderDock(); } else closeDock(); }
    else if (e.key === "Enter") { e.preventDefault(); submitDock(); }
    else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault(); const d = e.key === "ArrowDown" ? 1 : -1;
      if (mode === "cmd") { cm.sel = Math.max(0, Math.min(cm.sel + d, cm.items.length - 1)); renderDock(); }
      else if ((mode === "cli" || mode === "api") && cm.history.length) {
        cm.h = stepHistory(cm.h, cm.history.length, d < 0 ? "older" : "newer");
        input.value = cm.h === -1 ? input.value[0] : cm.history[cm.history.length - 1 - cm.h]; renderDock();
      }
    }
  });
  $("cmd-close").onclick = closeDock;
  $("cmd-clear").onclick = () => { cm.out = []; renderDock(); };
  // Capture phase: terminals stop propagation of the keys they handle.
  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "p") { e.preventDefault(); e.stopPropagation(); toggleDock(); }
  }, true);
}
