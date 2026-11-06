// The one place that launches a pattern run. Runs, Tareas and Patrones all use it, so the form, the
// target space, the task link and the agent-limit handling are identical everywhere.

// Starts a run. A pattern that needs more agents than `limits.max_concurrent_agents` is a deliberate
// request here, so the server's confirmation is given and the user is told, instead of failing.
async function startPatternRun(params) {
  const { pattern, task, root } = params;
  try {
    const run = await srv("shepherd.pattern.run.start", { pattern, task, root });
    scheduleRefresh(); return { run, overLimit: false };
  } catch (e) {
    if (!/confirmation_required: max_concurrent_agents/.test(String(e))) { ws.lastError = String(e); flash(String(e)); return null; }
  }
  try {
    const run = await srv("shepherd.pattern.run.start", { pattern, task, root, confirm_over_limit: true });
    scheduleRefresh(); return { run, overLimit: true };
  } catch (e) { ws.lastError = String(e); flash(String(e)); return null; }
}

// Form for launching a pattern in the active space.
//   task: a task object -> the objective starts with its text and the run is linked to it ("Tarea #N:")
//   pattern: a pattern name -> that one is fixed, no picker
//   onStarted(result, target): after a successful launch   onCancel(): shows a Cancelar button
function patternRunForm({ task = null, pattern = null, onStarted, onCancel }) {
  const pick = el("select", {});
  const objective = el("textarea", { className: "inline", rows: task ? 4 : 3, placeholder: "Objetivo del run (opcional)", value: task ? taskPrompt(task) : "" });
  const where = el("div", { className: "dim small" });
  const msg = el("div", { className: "dim small" });
  const go = el("button", { className: "btn small", onclick: async () => {
    if (shepherdOff()) { msg.textContent = "Activa Shepherd primero (aviso bajo los paneles)."; return; }
    if (!pick.value) { msg.textContent = "Elige un patrón."; return; }
    if (!ws.wid) { msg.textContent = "No hay ningún space activo."; return; }
    go.disabled = true; msg.textContent = "Preparando el space…";
    let target;
    try { target = await rootForWorkspace(ws.wid); } catch (e) { go.disabled = false; msg.textContent = `No se pudo preparar el space: ${e}`; return; }
    const text = task ? ensureTaskPrefix(objective.value, task) : objective.value.trim();
    msg.textContent = "Creando los agentes…";
    const res = await startPatternRun({ pattern: pick.value, task: text || undefined, root: target.root || undefined });
    go.disabled = false;
    if (!res) { msg.textContent = `No se pudo ejecutar: ${ws.lastError || "error desconocido"}`; return; }
    const n = res.run.run.agents.length;
    flash(`Patrón ${pick.value} ejecutado${task ? ` para la tarea #${task.id}` : ""}: ${n} agente(s)${res.overLimit ? " (supera el límite de agentes simultáneos)" : ""}${target.note ? ". " + target.note : ""}`);
    msg.textContent = ""; objective.value = task ? taskPrompt(task) : "";
    onStarted?.({ ...res, run: res.run.run }, target);
  } }, "Ejecutar patrón");
  const refreshWhere = () => {
    const w = ws.snap?.workspaces.find((x) => x.workspace_id === ws.wid);
    where.textContent = w ? `Se creará un agente por cada agente del patrón en el space «${w.label || "Workspace " + w.number}»${task ? " y el run quedará ligado a esta tarea" : ""}.` : "Crea un space antes de ejecutar un patrón.";
  };
  // Called on every redraw with the patterns of the project: keeps the choice, never the typed objective.
  const fill = (patterns) => {
    const chosen = pattern ?? pick.value;
    const shown = pattern ? patterns.filter((p) => p.name === pattern) : patterns;
    pick.replaceChildren(...(shown.length ? shown.map((p) => el("option", { value: p.name }, `${p.name} · ${p.kind} · ${p.agents_count} agente(s)`)) : [el("option", { value: "" }, "No hay patrones en este proyecto")]));
    if (shown.some((p) => p.name === chosen)) pick.value = chosen;
    pick.disabled = !!pattern; go.disabled = !shown.length; refreshWhere();
  };
  const node = el("div", { className: "aform" }, ...(pattern ? [] : [el("label", {}, "Patrón"), pick]),
    el("label", {}, task ? "Objetivo (empieza por «Tarea #N:» para ligar el run)" : "Objetivo"), objective, where, msg,
    el("div", { className: "row" }, go, ...(onCancel ? [el("button", { className: "mini", onclick: onCancel }, "Cancelar")] : [])));
  node.addEventListener("keydown", (e) => { e.stopPropagation(); if (e.key === "Escape") onCancel?.(); });
  fill([]);
  return { el: node, fill };
}
