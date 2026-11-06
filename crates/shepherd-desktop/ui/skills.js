// Skills tab, like the TUI: "Por Agente" (which skills an agent has active) and "Catálogo". Read-only.
let sk = { mode: 0, catalog: null, agents: [], agent: null, active: [], problems: [], sel: null };

const SCOPE = { default: "dim", user: "acc", repo: "ok" };
const scopeBadge = (s) => el("span", { className: "badge " + (SCOPE[s] ?? "dim") }, `[${s}]`);

async function drawSkills() {
  try {
    sk.catalog = await invoke("skill_catalog");
    sk.agents = await invoke("agent_list");
    if (!sk.agents.some((a) => a.id === sk.agent)) sk.agent = sk.agents[0]?.id ?? null;
    await loadAgentSkills();
  } catch (e) { return $("list").replaceChildren(el("div", { className: "empty" }, String(e))); }
  renderSkills();
}

async function loadAgentSkills() {
  sk.active = []; sk.problems = [];
  if (sk.mode !== 0 || !sk.agent) return;
  const r = await invoke("skill_agent", { agentId: sk.agent });
  sk.active = r.active; sk.problems = r.problems;
}

function skillRows() {
  const byName = new Map(sk.catalog.skills.map((s) => [s.name, s]));
  if (sk.mode === 1) return sk.catalog.skills.map((s) => ({ skill: s, title: s.name, meta: s.global ? "global" : "" }));
  const on = new Map(sk.active.map((a) => [a.name, a.level]));
  const rows = sk.catalog.skills.map((s) => ({ skill: s, active: on.has(s.name), level: on.get(s.name), title: `${on.has(s.name) ? "[✓]" : "[ ]"} ${s.name}` }));
  for (const [name, level] of on) if (!byName.has(name)) // recorded as installed but no longer in the catalog
    rows.push({ skill: { name, scope: "default", description: "Instalada en el agente pero ya no está en el catálogo.", problems: [] }, active: true, level, title: `[✓] ${name}`, meta: "no encontrada" });
  return rows;
}

function renderSkills() {
  const modes = el("div", { className: "filters" }, ...["Por Agente", "Catálogo"].map((m, i) =>
    el("button", { className: i === sk.mode ? "sel" : "", onclick: async () => { sk.mode = i; sk.sel = null; await loadAgentSkills(); renderSkills(); } }, m)));
  const head = [modes];
  if (sk.mode === 0 && sk.agents.length) {
    const pick = el("select", { className: "agentpick", onchange: async () => { sk.agent = pick.value; await loadAgentSkills(); renderSkills(); } },
      ...sk.agents.map((a) => el("option", { value: a.id, selected: a.id === sk.agent }, `${a.role} (${a.id})`)));
    head.push(pick);
  }
  const rows = sk.mode === 0 && !sk.agents.length ? [] : skillRows();
  if (!rows.some((r) => r.skill.name === sk.sel)) sk.sel = rows[0]?.skill.name ?? null;
  const items = el("div", { id: "items" });
  for (const r of rows)
    items.append(el("button", { className: "item" + (r.skill.name === sk.sel ? " sel" : "") + (sk.mode === 0 && !r.active ? " off" : ""), onclick: () => { sk.sel = r.skill.name; renderSkills(); } },
      el("span", {}, r.title + " ", scopeBadge(r.skill.scope)), el("small", {}, r.meta ?? "")));
  $("list").replaceChildren(...head, items, el("button", { id: "new", onclick: drawSkills }, "Refrescar"));

  const row = rows.find((r) => r.skill.name === sk.sel);
  if (!row) return $("detail").replaceChildren(el("div", { className: "empty" }, sk.mode === 0 && !sk.agents.length
    ? "No hay agentes en el registro: usa el Catálogo para ver las skills disponibles." : "No hay skills."));
  const s = row.skill, block = (t, body) => el("div", { className: "block" }, el("h4", {}, t), el("pre", {}, body));
  const detail = el("div", { className: "blocks" });
  const badges = [scopeBadge(s.scope)];
  let meta;
  if (sk.mode === 0) {
    badges.push(el("span", { className: "badge " + (row.active ? "ok" : "dim") }, row.active ? "[activa]" : "[inactiva]"));
    meta = `Agente: ${sk.agent} | Ámbito: ${s.scope} | Nivel: ${row.level ?? "—"}`;
    detail.append(block("DESCRIPCIÓN:", s.description));
    detail.append(el("div", { className: "info" }, "AVISO DE APLICACIÓN:\n• Añadir o quitar skills surte efecto al REINICIAR el agente.\n• Editar el texto de la skill se propaga EN VIVO inmediatamente."));
    if (sk.problems.length) detail.append(el("div", { className: "notice" }, "Problemas al resolver las skills:\n" + sk.problems.join("\n")));
  } else {
    if (s.global) badges.push(el("span", { className: "badge acc" }, "[global]"));
    if (!s.enabled) badges.push(el("span", { className: "badge bad" }, "[con errores]"));
    meta = `Ámbito: ${s.scope} | Global: ${s.global ? "Sí" : "No"} | Habilitada: ${s.enabled ? "Sí" : "No"} | ${s.dir}`;
    detail.append(block("DESCRIPCIÓN:", s.description));
    if ((s.problems ?? []).length) detail.append(el("div", { className: "notice" }, "Problemas detectados:\n" + s.problems.join("\n")));
    detail.append(block("SKILL.md:", s.body));
  }
  $("detail").replaceChildren(el("div", { className: "head" }, `SKILL: ${s.name}`, ...badges, el("span", { className: "state" })),
    el("div", { className: "meta" }, meta), detail);
}
