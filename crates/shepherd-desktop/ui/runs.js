// Runs tab: persisted pattern runs, same list + detail layout as the TUI. Read-only.
let rn = { list: [], sel: null, raw: "" }; // sel: run id

const runBadge = (s) => ({ running: ["[RUNNING]", "ok"], paused: ["[PAUSED]", "warn"], stalled: ["[STALLED]", "warn"],
  degraded: ["[DEGRADED]", "warn"], failed: ["[FAILED]", "bad"], done: ["[DONE]", "dim"], stopped: ["[STOPPED]", "dim"] }[s] ?? [`[${String(s).toUpperCase()}]`, "dim"]);

function applyRuns(raw) {
  rn.raw = raw; rn.list = JSON.parse(raw);
  if (!rn.list.some((r) => r.id === rn.sel)) rn.sel = rn.list[0]?.id ?? null;
  renderRuns();
}

async function drawRuns() {
  poll("runs", refreshRuns); // runs live in a file
  try { applyRuns(JSON.stringify(await invoke("run_list"))); }
  catch (e) { $("list").replaceChildren(el("div", { className: "empty" }, String(e))); $("detail").replaceChildren(); }
}

async function refreshRuns() {
  try { const raw = JSON.stringify(await invoke("run_list")); if (raw !== rn.raw) applyRuns(raw); } catch (_) { /* keep last view */ }
}

function renderRuns() {
  const items = el("div", { id: "items" });
  for (const r of rn.list) {
    const [txt, cls] = runBadge(r.state);
    items.append(el("button", { className: "item" + (r.id === rn.sel ? " sel" : ""), onclick: () => { rn.sel = r.id; renderRuns(); } },
      el("span", {}, `${r.pattern} (${r.id}) `, el("span", { className: "badge " + cls }, txt)),
      el("small", {}, `${r.live_agents}/${(r.agents ?? []).length} agentes`)));
  }
  $("list").replaceChildren(items, el("button", { id: "new", onclick: refreshRuns }, "Refrescar"));
  const r = rn.list.find((x) => x.id === rn.sel);
  if (!r) return $("detail").replaceChildren(el("div", { className: "empty" },
    "No hay runs de patrones. Se lanzan desde la TUI (pestaña Patrones, tecla r) o con `shepherd pattern run`."));

  const [txt, cls] = runBadge(r.state);
  const block = (title, body) => el("div", { className: "block" }, el("h4", {}, title), el("pre", {}, body));
  const detail = el("div", { className: "blocks" });
  if (r.stalled_reason) detail.append(el("div", { className: "notice" }, "RUN ATASCADO:\n" + r.stalled_reason));
  detail.append(block("OBJETIVO:", r.task || "Sin objetivo."));
  if (r.error) detail.append(block("ERROR:", r.error));
  detail.append(block("AGENTES:", (r.agents ?? []).map((a) =>
    `${a.start ? "▶ " : "  "}${a.id} (${a.role}) [${a.state}]${[a.vendor, a.model].filter(Boolean).length ? " " + [a.vendor, a.model].filter(Boolean).join("/") : ""}`).join("\n") || "Ninguno."));
  detail.append(block("TÓPICOS:", (r.topics ?? []).join(", ") || "Ninguno."));
  if (r.quiesced) detail.append(block("NOTA:", "Terminó por inactividad; un mensaje nuevo en sus tópicos lo reabre."));
  const meta = [`Topología: ${r.kind}`, `Creado: ${r.created_at}`, `Actualizado: ${r.updated_at}`, r.finished_at && `Terminado: ${r.finished_at}`, r.worktree && `Worktree: ${r.worktree}`].filter(Boolean);
  $("detail").replaceChildren(
    el("div", { className: "head" }, `RUN: ${r.pattern} (ID: ${r.id})`, el("span", { className: "badge " + cls }, txt), el("span", { className: "state" })),
    el("div", { className: "meta" }, meta.join(" | ")), detail);
}
