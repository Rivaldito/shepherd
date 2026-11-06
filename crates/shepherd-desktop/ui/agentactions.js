// Agent actions over the server API: launch (role + task), kill, clean, resume, prompt.
// They change real state, so killing/cleaning/resuming need two clicks (no pop-ups).

const TERMINAL_STATES = ["done", "exited", "killed", "failed"];

// Model field shared by both launch modes: a text input with a datalist of suggestions, and the
// optional button that saves the detected models into the vendor's config.
function modelPicker() {
  const input = el("input", { type: "text", className: "inline", placeholder: "modelo (opcional)" });
  const list = el("datalist", { id: "models-" + Math.random().toString(36).slice(2) });
  input.setAttribute("list", list.id);
  const hint = el("div", { className: "dim small" });
  const saveBtn = el("button", { className: "btn small", hidden: true }, "");
  let ctx = null, confirming = false;

  async function show(kind, vendorName, refresh = false) {
    ctx = { kind, vendorName }; confirming = false; saveBtn.hidden = true;
    if (!kind) { list.replaceChildren(); hint.textContent = ""; return; }
    hint.textContent = "buscando modelos…";
    const info = await invoke("agent_models", { kind, refresh }).catch((e) => ({ live: [], live_error: String(e), agent_default: null }));
    if (ctx.kind !== kind) return; // the selection moved on while we waited
    const vendor = dc.vendors[vendorName ?? vendorForKind(kind)] ?? null;
    const configured = vendor?.models ?? [];
    list.replaceChildren(...mergeModels({ live: info.live, configured, agentDefault: info.agent_default }).map((o) => el("option", { value: o.id }, o.label)));
    input.placeholder = `modelo (opcional)${vendor?.default_model || info.agent_default ? " · por defecto: " + (vendor?.default_model || info.agent_default) : ""}`;
    const parts = [];
    if (info.live.length) parts.push(`${info.live.length} detectados`);
    if (configured.length) parts.push(`${configured.length} en la configuración`);
    if (info.live_error) parts.push(`no se pudieron listar: ${info.live_error}`);
    else if (!info.live.length && !configured.length) parts.push("este agente no lista sus modelos: escribe el nombre");
    hint.textContent = parts.join(" · ");
    const fresh = vendor ? newModelIds(info.live, configured) : [];
    ctx.save = fresh.length ? { name: vendorName ?? vendorForKind(kind), vendor, models: [...configured, ...fresh] } : null;
    saveBtn.hidden = !ctx.save;
    saveBtn.textContent = ctx.save ? `Guardar ${fresh.length} modelo(s) detectado(s) en la configuración` : "";
    saveBtn.classList.remove("danger");
  }

  saveBtn.onclick = async () => {
    const save = ctx?.save; if (!save) return;
    if (!confirming) { confirming = true; saveBtn.classList.add("danger"); saveBtn.textContent = "¿Escribir en config.yaml? (se pierden sus comentarios; queda copia .bak)"; return; }
    confirming = false;
    try {
      await invoke("save_vendor", { name: save.name, vendor: { ...save.vendor, models: save.models } });
      flash(`Modelos de «${save.name}» guardados en la configuración`);
      await loadDiscovery(); await show(ctx.kind, ctx.vendorName);
    } catch (e) { flash(String(e)); }
  };
  return { input, list, hint, saveBtn, show };
}

const optionFor = (value, text, disabled = false) => el("option", { value, disabled }, text);

// Launch form with two modes: an agent with a Shepherd role and task, or a plain agent of any
// installed kind. `root` is the folder to work in; `onDone({ id?, pane })` runs after a launch.
function spawnForm({ root = "", onDone, onCancel }) {
  let mode = "role";
  const msg = el("div", { className: "dim" });
  const body = el("div", { className: "aform-body" });
  const seg = el("div", { className: "seg" });
  const picker = modelPicker();

  // --- role mode
  const role = el("select", { className: "inline" }, optionFor("", "cargando roles…"));
  const task = el("textarea", { className: "inline", rows: 3, placeholder: "Tarea del agente" });
  const dir = el("input", { type: "text", className: "inline", placeholder: "carpeta del proyecto", value: root });
  const vendor = el("select", { className: "inline" }, optionFor("", "(el vendor del rol)"));
  const worktree = el("input", { type: "checkbox" });
  // --- plain mode
  const kind = el("select", { className: "inline" });
  const nick = el("input", { type: "text", className: "inline", placeholder: "nombre (a-z, 0-9, - y _; máx. 32)" });

  const fillVendors = () => {
    vendor.replaceChildren(optionFor("", "(el vendor del rol)"), ...Object.entries(dc.vendors).sort(([, a], [, b]) => Number(!!pathOf(b.command)) - Number(!!pathOf(a.command))).map(([name, v]) =>
      optionFor(name, `${name} · ${v.command}${pathOf(v.command) ? " ✓" : " — no instalado"}`)));
  };
  const fillKinds = () => {
    kind.replaceChildren(...kindChoices().map((d) => optionFor(d.command, d.path ? `${d.command} ✓` : `${d.command} — no instalado`, !d.path)));
    if (!kind.options.length || kind.selectedOptions[0]?.disabled) kind.value = "";
  };
  const roleVendor = () => dc.roles[role.value]?.vendor || null;
  const refreshModels = () => {
    if (mode === "plain") return picker.show(kind.value || null, vendorForKind(kind.value));
    const name = vendor.value || roleVendor();
    return picker.show(name && dc.vendors[name] ? kindOfVendor(dc.vendors[name]) : null, name);
  };
  role.onchange = refreshModels; vendor.onchange = refreshModels; kind.onchange = refreshModels;

  const submit = el("button", { className: "btn small", onclick: launch }, "Lanzar");

  function render() {
    seg.replaceChildren(...[["role", "Con rol de Shepherd"], ["plain", "Agente suelto"]].map(([m, label]) =>
      el("button", { className: "mini" + (m === mode ? " on" : ""), onclick: () => { mode = m; msg.textContent = ""; render(); refreshModels(); } }, label)));
    submit.textContent = mode === "role" ? "Lanzar" : "Iniciar";
    const refresh = el("button", { className: "mini", title: "Volver a buscar agentes instalados y sus modelos", onclick: async () => { msg.textContent = "Buscando…"; await loadDiscovery(true); fillVendors(); fillKinds(); fillRoles(); await refreshModels(); msg.textContent = ""; } }, "Detectar de nuevo");
    body.replaceChildren(...(mode === "role"
      ? [el("label", {}, "Rol"), role, el("label", {}, "Tarea"), task, el("label", {}, "Carpeta"), dir, el("label", {}, "Vendor"), vendor,
        el("label", {}, "Modelo"), picker.input, picker.list, picker.hint, picker.saveBtn, el("label", { className: "check" }, worktree, " en un worktree propio")]
      : [el("label", {}, "Agente"), kind, el("label", {}, "Nombre"), nick, el("label", {}, "Modelo"), picker.input, picker.list, picker.hint, picker.saveBtn]), refresh);
  }

  const fillRoles = () => {
    const names = Object.keys(dc.roles);
    role.replaceChildren(...(names.length ? names.map((n) => optionFor(n, n)) : [optionFor("", "sin roles")]));
    if (!names.length) msg.textContent = "No hay roles configurados: créalos en la pestaña Modelos del panel Shepherd.";
  };

  async function launch() {
    const model = picker.input.value.trim();
    if (mode === "plain") {
      if (!kind.value) { msg.textContent = "Elige un agente instalado."; return; }
      submit.disabled = true; msg.textContent = "Iniciando…";
      const typed = nick.value.trim(), name = sanitizeAgentName(typed);
      if (typed && !name) { msg.textContent = "El nombre debe empezar por una letra (a-z, 0-9, - y _)."; return; }
      if (typed && name !== typed) { nick.value = name; flash(`Nombre ajustado a «${name}»`); }
      const started = await quickStartAgent({ kind: kind.value, name, model });
      submit.disabled = false;
      if (!started) { msg.textContent = "No se pudo iniciar (ver la barra inferior)."; return; }
      flash(`Agente ${kind.value} iniciado`); onDone?.(started); return;
    }
    if (!role.value || !task.value.trim()) { msg.textContent = "Elige un rol y escribe la tarea."; return; }
    const params = { role: role.value, task: task.value.trim(), worktree: worktree.checked };
    if (dir.value.trim()) params.root = dir.value.trim();
    if (vendor.value) params.vendor = vendor.value;
    if (model) params.model = model;
    submit.disabled = true; msg.textContent = "Lanzando…";
    const r = await act("shepherd.agent.spawn", params);
    submit.disabled = false;
    if (!r) { msg.textContent = "No se pudo lanzar (ver la barra inferior)."; return; }
    flash(`Agente ${r.agent.id} lanzado`);
    onDone?.(r.agent);
  }

  const form = el("div", { className: "aform" }, seg, body, msg, el("div", { className: "row" }, submit, el("button", { className: "mini", onclick: () => onCancel?.() }, "Cancelar")));
  form.addEventListener("keydown", (e) => { e.stopPropagation(); if (e.key === "Escape") onCancel?.(); });
  render();
  loadDiscovery().then(() => { fillVendors(); fillKinds(); fillRoles(); render(); refreshModels(); });
  return form;
}

// Starts a plain agent (no role) in a new tab of the active space: `agent.start` needs a pane that
// sits at a shell prompt. Returns { pane } or null (the failure is already in the status bar).
async function quickStartAgent({ kind, name, model }) {
  const cwd = activeCwd() || undefined;
  let pane = null;
  if (ws.wid) {
    const r = await act("tab.create", { workspace_id: ws.wid, focus: true, cwd });
    if (r) { ws.tid = r.tab.tab_id; pane = r.root_pane.pane_id; }
  } else {
    const r = await act("workspace.create", { focus: true, cwd });
    if (r) { ws.wid = r.workspace.workspace_id; ws.tid = r.tab.tab_id; pane = r.root_pane.pane_id; }
  }
  if (!pane) return null;
  const flag = modelFlagFor(kind), args = model && flag ? [flag, model] : undefined;
  // Two failures are worth retrying: a fresh pane whose shell is not ready yet (a moment), and a name
  // taken since we looked (the next free one). Anything else is reported as it is.
  const taken = () => new Set(ws.snap?.agents.map((x) => x.name).filter(Boolean));
  let started = null, lastError = "";
  for (let attempt = 0; attempt < 14 && !started; attempt++) {
    const agentName = name || uniqueName(kind, taken());
    try { started = await srv("agent.start", { name: agentName, kind, pane_id: pane, args, timeout_ms: 30000 }); }
    catch (e) {
      lastError = String(e);
      if (lastError.includes("agent_pane_busy")) { await new Promise((r) => setTimeout(r, 300)); continue; }
      if (!name && lastError.includes("agent_name_taken")) { await refreshSnapshot(); continue; }
      break;
    }
  }
  if (!started) { flash(lastError); return null; }
  scheduleRefresh();
  return { pane };
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
