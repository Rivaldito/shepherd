const { invoke } = window.__TAURI__.core;
const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...kids) => { const e = Object.assign(document.createElement(tag), props); e.append(...kids); return e; };
const get = (o, p) => p.split(".").reduce((a, k) => a?.[k], o);
const set = (o, p, v) => { const ks = p.split("."); const last = ks.pop(); ks.reduce((a, k) => (a[k] ??= {}), o)[last] = v; };

// [path, kind, hint]; kinds: text | lines (one per line) | bool | area
const VENDOR = [["command","text"],["args","lines"],["models","lines"],["default_model","text"],["model_flag","text"],
  ["print_args","lines"],["mcp.supported","bool"],["mcp.style","text"],["skills.mode","text"],["skills.style","text"],
  ["skills.file","text"],["shepherd_agent","text"],["disabled","bool"]];
const ROLE = [["vendor","text"],["model","text"],["fallback","lines"],["must_differ_from","text"],
  ["worktree","bool"],["can_spawn","bool"],["skills","lines"],["prompt","area"]];
const SPECS = { vendors: VENDOR, roles: ROLE };
const TABS = [["agents", "Agentes"], ["tasks", "Tareas"], ["runs", "Runs"], ["models", "Modelos"], ["patterns", "Patrones"], ["skills", "Skills"], ["git", "Git"], ["events", "Eventos"], ["files", "Archivos"]];

let tab = "agents", files = [], models = null, sel = null; // sel: {section,name,isNew} | file entry
let dirty = false, armed = null;

// One poller for the file-backed tabs (agents, runs, tasks): re-runs `fn` every 3 s while `key` is the open tab.
let pollTimer = 0;
function poll(key, fn) {
  clearInterval(pollTimer);
  pollTimer = setInterval(() => (tab !== key ? clearInterval(pollTimer) : $("shepherd-panel").hidden || fn()), 3000);
}

const setMsg = (box, text, cls = "") => { box.textContent = text; box.className = cls; };

// Unsaved-changes guard without pop-ups: first attempt warns, second one discards.
function leave(key, box) {
  if (dirty && armed !== key) { armed = key; setMsg(box, "Cambios sin guardar: guarda, o repite la acción para descartarlos.", "bad"); return false; }
  armed = null; dirty = false; return true;
}
const msgBox = () => $("msgs") || document.createElement("pre");

function drawTabs() {
  $("tabs").replaceChildren(...TABS.map(([k, label], i) => {
    const b = el("button", { className: k === tab ? "sel" : "", onclick: () => { if (leave("tab:" + k, msgBox())) { tab = k; sel = null; pe = null; draw(); } } },
      el("b", {}, String(i + 1)), label);
    return b;
  }));
}

function listItem({ title, meta, off, active, onclick }) {
  return el("button", { className: "item" + (active ? " sel" : "") + (off ? " off" : ""), onclick }, title, el("small", {}, meta ?? ""));
}

function pick(next) {
  if (!leave("item:" + JSON.stringify(next), msgBox())) return;
  sel = next; draw();
}

async function draw() {
  drawTabs();
  if (tab === "agents") return drawAgents();
  if (tab === "tasks") return drawTasks();
  if (tab === "runs") return drawRuns();
  if (tab === "skills") return drawSkills();
  if (tab === "git") return drawGit();
  if (tab === "events") return drawEvents();
  if (tab === "models") { models = await invokeP("get_models").catch((e) => { models = null; sel = null; $("list").replaceChildren(el("div", { className: "empty" }, String(e))); return null; }); if (!models) return; }
  else if (tab === "patterns") return drawPatterns();
  else files = await invokeP("list_files");

  const items = el("div", { id: "items" });
  if (tab === "models") {
    for (const [section, title, meta] of [["vendors", "Vendors", (v) => v.disabled ? "desactivado" : v.default_model], ["roles", "Roles", (r) => [r.vendor, r.model].filter(Boolean).join("/")]]) {
      items.append(el("h3", {}, title));
      for (const [name, v] of Object.entries(models[section]))
        items.append(listItem({ title: name, meta: meta(v), off: v.disabled, active: sel?.section === section && sel.name === name && !sel.isNew, onclick: () => pick({ section, name }) }));
    }
    if (sel?.isNew) items.append(listItem({ title: "(nuevo " + (sel.section === "vendors" ? "vendor" : "rol") + ")", active: true, onclick: () => {} }));
    $("list").replaceChildren(items,
      el("button", { id: "new", onclick: () => pick({ section: "vendors", name: "", isNew: true }) }, "+ Nuevo vendor"),
      el("button", { id: "new", onclick: () => pick({ section: "roles", name: "", isNew: true }) }, "+ Nuevo rol"));
    return sel ? drawForm() : $("detail").replaceChildren(el("div", { className: "empty" }, "Selecciona un vendor o un rol."));
  }
  for (const [g, title] of [["config", "Configuración"], ["patterns", "Patrones (YAML)"]]) {
    items.append(el("h3", {}, title));
    for (const f of files.filter((x) => x.group === g))
      items.append(listItem({ title: f.label, meta: f.exists ? "" : "nuevo", active: sel?.path === f.path, onclick: () => pick(f) }));
  }
  $("list").replaceChildren(items);
  sel && sel.path ? drawRaw() : $("detail").replaceChildren(el("div", { className: "empty" }, "Selecciona un archivo."));
}

function drawForm() {
  const { section, isNew } = sel;
  const draft = isNew ? {} : structuredClone(models[section][sel.name]);
  const msgs = el("pre", { id: "msgs" }, `Se guarda en ${models.target}`);
  const state = el("span", { className: "state" });
  const form = el("div", { className: "form" });
  const inputs = [];
  const mark = () => { dirty = true; state.textContent = "● sin guardar"; };

  let nameInput = null;
  if (isNew) { nameInput = el("input", { type: "text", placeholder: "nombre", oninput: mark }); form.append(el("label", {}, "nombre"), nameInput); }
  for (const [path, kind] of SPECS[section]) {
    const v = get(draft, path);
    let input;
    if (kind === "bool") input = el("input", { type: "checkbox", checked: !!v, onchange: mark });
    else if (kind === "text") input = el("input", { type: "text", value: v ?? "", oninput: mark });
    else input = el("textarea", { value: kind === "lines" ? (v ?? []).join("\n") : (v ?? ""), rows: kind === "area" ? 6 : 2, oninput: mark });
    inputs.push([path, kind, input]);
    form.append(el("label", {}, path), input);
  }

  const collect = () => {
    for (const [path, kind, i] of inputs)
      set(draft, path, kind === "bool" ? i.checked : kind === "lines" ? i.value.split("\n").map((s) => s.trim()).filter(Boolean) : i.value);
    return draft;
  };
  const report = (w) => setMsg($("msgs"), w.length ? w.join("\n") : "Guardado, sin problemas.", w.length ? "bad" : "ok");
  const save = el("button", { className: "btn primary", onclick: async () => {
    const name = isNew ? nameInput.value.trim() : sel.name;
    try {
      const w = await invoke(section === "vendors" ? "save_vendor" : "save_role", { name, [section === "vendors" ? "vendor" : "role"]: collect() });
      dirty = false; sel = { section, name }; await draw(); report(w);
    } catch (e) { setMsg(msgs, String(e), "bad"); }
  } }, "Guardar");
  const head = el("div", { className: "head" }, (section === "vendors" ? "VENDOR: " : "ROL: ") + (isNew ? "nuevo" : sel.name), state, save);
  if (!isNew) {
    let sure = false;
    const del = el("button", { className: "btn danger", onclick: async () => {
      if (!sure) { sure = true; del.textContent = "¿Seguro? Pulsa otra vez"; return; }
      try { const w = await invoke("delete_entry", { section, name: sel.name }); dirty = false; sel = null; await draw(); report(w); }
      catch (e) { setMsg(msgs, String(e), "bad"); }
    } }, "Eliminar");
    head.append(del);
  }
  $("detail").replaceChildren(head, form, msgs);
}

async function drawRaw() {
  const f = sel;
  const msgs = el("pre", { id: "msgs" }, f.path);
  const state = el("span", { className: "state" });
  const ed = el("textarea", { id: "editor", spellcheck: false, value: await invokeP("read_file", { path: f.path }) });
  const save = el("button", { className: "btn primary", disabled: true, onclick: async () => {
    try {
      const w = await invokeP("save_file", { path: f.path, text: ed.value });
      dirty = false; state.textContent = ""; save.disabled = true;
      setMsg(msgs, w.length ? w.join("\n") : "Guardado, sin problemas.", w.length ? "bad" : "ok");
    } catch (e) { setMsg(msgs, String(e), "bad"); }
  } }, "Guardar (Ctrl+S)");
  ed.oninput = () => { dirty = true; state.textContent = "● sin guardar"; save.disabled = false; };
  $("detail").replaceChildren(el("div", { className: "head" }, f.label, state, save), ed, msgs);
}

addEventListener("keydown", (e) => {
  if ($("shepherd-panel").hidden) return; // terminals own the keyboard
  if (e.ctrlKey && e.key === "s") { e.preventDefault(); [...document.querySelectorAll(".head .btn.primary")][0]?.click(); }
  if (!e.ctrlKey && !e.altKey && !/INPUT|TEXTAREA/.test(document.activeElement.tagName)) {
    const t = TABS[Number(e.key) - 1]; if (t) document.querySelectorAll("#tabs button")[Number(e.key) - 1].click();
  }
});
draw();
initStatus();
initWorkspace();
initCommands();
