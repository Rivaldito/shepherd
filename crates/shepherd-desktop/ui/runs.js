// Runs tab: persisted pattern runs, same list + detail layout as the TUI. Can launch a pattern and
// pause / resume / stop a run, like `shepherd pattern run|pause|resume|stop`.
let rn = { list: [], sel: null, raw: "", patterns: [], launching: false, form: null, confirmStop: null, busy: false }; // sel: run id

const runBadge = (s) => ({ running: ["[RUNNING]", "ok"], paused: ["[PAUSED]", "warn"], stalled: ["[STALLED]", "warn"],
  degraded: ["[DEGRADED]", "warn"], failed: ["[FAILED]", "bad"], done: ["[DONE]", "dim"], stopped: ["[STOPPED]", "dim"] }[s] ?? [`[${String(s).toUpperCase()}]`, "dim"]);

function applyRuns(raw) {
  rn.raw = raw; rn.list = JSON.parse(raw);
  if (!rn.list.some((r) => r.id === rn.sel)) rn.sel = rn.list[0]?.id ?? null;
  renderRuns();
}

async function drawRuns() {
  poll("runs", refreshRuns); // runs live in a file
  rn.patterns = await invokeP("pattern_list").catch(() => []);
  try { applyRuns(JSON.stringify(await invokeP("run_list"))); }
  catch (e) { $("list").replaceChildren(el("div", { className: "empty" }, String(e))); $("detail").replaceChildren(); }
}

async function refreshRuns() {
  try { const raw = JSON.stringify(await invokeP("run_list")); if (raw !== rn.raw) applyRuns(raw); } catch (_) { /* keep last view */ }
}

// Built once (launcher.js) so a refresh never wipes the objective being typed.
function runLauncher() {
  return patternRunForm({ onCancel: () => { rn.launching = false; renderRuns(); },
    onStarted: async ({ run }) => { rn.launching = false; rn.sel = run.id; await refreshRuns(); } });
}

// pause / resume / stop; stopping asks for a second click on the same button (no pop-ups).
async function controlRun(r, action) {
  if (action === "stop" && rn.confirmStop !== r.id) { rn.confirmStop = r.id; return renderRuns(); }
  rn.confirmStop = null; rn.busy = true; renderRuns();
  const root = projectRoot() || undefined;
  const res = action === "pause" ? await act("shepherd.pattern.run.pause", { id: r.id, root })
    : action === "resume" ? await act("shepherd.pattern.run.resume", { id: r.id, root })
    : await act("shepherd.pattern.run.stop", { id: r.id, root, confirm: true });
  rn.busy = false;
  if (!res) flash(`No se pudo ${{ pause: "pausar", resume: "reanudar", stop: "detener" }[action]}: ${ws.lastError || "error desconocido"}`);
  await refreshRuns(); renderRuns();
}

function runControls(r) {
  const live = ["running", "stalled", "degraded"].includes(r.state);
  const btn = (label, action, title) => el("button", { className: "btn small", disabled: rn.busy, title, onclick: () => controlRun(r, action) }, label);
  return [
    live ? btn("Pausar", "pause", "Los agentes dejan de recibir mensajes del bus") : "",
    r.state === "paused" ? btn("Reanudar", "resume", "Vuelve a entregar los mensajes pendientes") : "",
    live || r.state === "paused" ? btn(rn.confirmStop === r.id ? "Confirmar: detener y cerrar agentes" : "Detener", "stop", "Cierra los agentes del run") : "",
  ];
}

function renderRuns() {
  const items = el("div", { id: "items" });
  for (const r of rn.list) {
    const [txt, cls] = runBadge(r.state);
    items.append(el("button", { className: "item" + (r.id === rn.sel ? " sel" : ""), onclick: () => { rn.sel = r.id; renderRuns(); } },
      el("span", {}, `${r.pattern} (${r.id}) `, el("span", { className: "badge " + cls }, txt)),
      el("small", {}, `${r.live_agents}/${(r.agents ?? []).length} agentes`)));
  }
  $("list").replaceChildren(items, el("div", { className: "panel-actions" },
    el("button", { className: "btn small", onclick: () => { rn.launching = true; rn.form ??= runLauncher(); renderRuns(); } }, "+ Ejecutar patrón"),
    el("button", { className: "btn small", onclick: refreshRuns }, "Refrescar")));
  if (rn.launching) {
    rn.form.fill(rn.patterns);
    return $("detail").replaceChildren(el("div", { className: "head" }, "EJECUTAR PATRÓN", el("span", { className: "state" })), el("div", { className: "blocks" }, rn.form.el));
  }
  const r = rn.list.find((x) => x.id === rn.sel);
  if (!r) return $("detail").replaceChildren(el("div", { className: "empty" },
    "No hay runs de patrones. Ejecuta uno con «+ Ejecutar patrón» (equivale a `shepherd pattern run`)."));

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
    el("div", { className: "head" }, `RUN: ${r.pattern} (ID: ${r.id})`, el("span", { className: "badge " + cls }, txt), el("span", { className: "state" }),
      ...runControls(r),
      el("button", { className: "btn small", onclick: () => showMonitorTab(`run:${r.id}`) }, "Ver comunicación"),
      el("button", { className: "btn small", onclick: () => openMonitorWindow(`run:${r.id}`) }, "Ventana nueva")),
    el("div", { className: "meta" }, meta.join(" | ")), detail);
}
