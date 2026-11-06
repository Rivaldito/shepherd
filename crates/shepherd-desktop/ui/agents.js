// Agents tab, laid out like the TUI: list (role (id), state badge, vendor / NEEDS YOU) + detail.
// Read-only: spawning/killing agents changes live state and stays in the TUI/CLI.
let ag = { list: [], sel: null, raw: "" }; // sel: agent id

const needsYou = (a) => ["blocked", "needs-human"].includes(a.state) || !!(a.notice ?? "").trim();
const stateBadge = (s) => {
  if (["working", "running"].includes(s)) return ["[WORKING]", "ok"];
  if (["blocked", "needs-human"].includes(s)) return ["[BLOCKED]", "warn"];
  if (["done", "completed"].includes(s)) return ["[DONE]", "dim"];
  if (s === "failed") return ["[FAILED]", "bad"];
  if (s === "killed") return ["[KILLED]", "bad"];
  return ["[IDLE]", "dim"];
};

async function drawAgents() {
  poll("agents", refreshAgents); // the registry is a file
  let raw;
  try { raw = JSON.stringify(await invoke("agent_list")); }
  catch (e) { return $("list").replaceChildren(el("div", { className: "empty" }, String(e))); }
  ag.raw = raw; ag.list = JSON.parse(raw);
  if (!ag.list.some((a) => a.id === ag.sel)) ag.sel = ag.list[0]?.id ?? null;
  renderAgents();
}

async function refreshAgents() {
  try {
    const raw = JSON.stringify(await invoke("agent_list"));
    if (raw !== ag.raw) { ag.raw = raw; ag.list = JSON.parse(raw); if (!ag.list.some((a) => a.id === ag.sel)) ag.sel = ag.list[0]?.id ?? null; renderAgents(); }
  } catch (_) { /* keep the last good view */ }
}

function renderAgents() {
  const items = el("div", { id: "items" });
  for (const a of ag.list) {
    const [txt, cls] = stateBadge(a.state);
    const b = el("button", { className: "item" + (a.id === ag.sel ? " sel" : ""), onclick: () => { ag.sel = a.id; renderAgents(); } },
      el("span", {}, `${a.role} (${a.id}) `, el("span", { className: "badge " + cls }, txt)),
      el("small", {}, needsYou(a) ? "⚠ NEEDS YOU" : a.vendor));
    items.append(b);
  }
  $("list").replaceChildren(items, el("button", { id: "new", onclick: refreshAgents }, "Refrescar"));
  const a = ag.list.find((x) => x.id === ag.sel);
  if (!a) return $("detail").replaceChildren(el("div", { className: "empty" },
    "No hay agentes en el registro. Se lanzan desde la TUI (pestaña Agentes, tecla s)."));

  const [txt, cls] = stateBadge(a.state);
  const block = (title, body) => el("div", { className: "block" }, el("h4", {}, title), el("pre", {}, body));
  const detail = el("div", { className: "blocks" });
  if (needsYou(a)) detail.append(el("div", { className: "notice" }, "AVISO DESTACADO (NEEDS YOU):\nEsperando al humano: " +
    ((a.notice ?? "").trim() || "Acción requerida en el terminal nativo (diálogo, login o confirmación).")));
  detail.append(block("TAREA ASIGNADA:", a.task_text || "Sin descripción de tarea asignada."));
  const info = [["Worktree", a.worktree], ["Branch", a.branch], ["Padre", a.parent_id], ["Raíz", a.root], ["Tarea", a.task_id],
    ["Skills activas", (a.active_skills ?? []).join(", ")], ["Iniciado", a.started], ["Actualizado", a.updated]].filter(([, v]) => v);
  if (info.length) detail.append(block("DETALLES:", info.map(([k, v]) => `${k}: ${v}`).join("\n")));
  if (a.result) detail.append(block("RESULTADO:", a.result));
  if (a.error) detail.append(block("ERROR:", a.error));
  detail.append(block("SNAPSHOTS:", a.snapshots ? `${a.snapshots} snapshot(s); último: ${a.last_snapshot}.` : "Aún no hay snapshots."));
  $("detail").replaceChildren(
    el("div", { className: "head" }, `AGENTE: ${a.role} (ID: ${a.id})`, el("span", { className: "badge " + cls }, txt), el("span", { className: "state" })),
    el("div", { className: "meta" }, `Vendor: ${a.vendor} | Modelo: ${a.model || "default"} | Terminal Pane: ${a.pane || "ninguno"}`),
    detail);
}
