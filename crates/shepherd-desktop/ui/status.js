// Bottom status bar: connection to the Shepherd server (Herdr shows machine · branch · agents here).
// Starting a server is an explicit button: it creates real state, so the app never does it by itself.
let sb = { status: null, note: "", busy: false, shepherd: "unknown", enabling: false, cfg: null }; // shepherd: on | off | unknown

// The orchestration layer (roles, tasks, patterns) is off by default and only starts with the server.
const shepherdOff = () => !!sb.status?.running && sb.shepherd === "off";

async function refreshStatus() {
  try { sb.status = await invoke("api_status"); }
  catch (e) { sb.status = { running: false, socket: "", server_bin: null }; sb.note = String(e); }
  sb.shepherd = "unknown";
  if (sb.status.running) {
    try { await invoke("api_call", { method: "shepherd.agent.list", params: {} }); sb.shepherd = "on"; }
    catch (e) { sb.shepherd = String(e).includes("shepherd_not_running") ? "off" : "on"; }
  }
  renderStatus();
}

async function startServer() {
  sb.busy = true; sb.note = "Arrancando el servidor…"; renderStatus();
  try { sb.status = await invoke("api_start_server"); sb.note = ""; }
  catch (e) { sb.note = String(e); }
  sb.busy = false; renderStatus();
  if (typeof onServerConnected === "function") onServerConnected();
}

function renderStatus() {
  const s = sb.status; if (!s) return;
  const bar = $("statusbar");
  const left = [el("span", { className: "dot " + (s.running ? "ok" : "bad") }, s.running ? "●" : "○"),
    el("span", {}, s.running ? `Servidor Shepherd ${s.version ?? ""} · protocolo ${s.protocol ?? "?"}` : "Sin servidor"),
    el("span", { className: "dim", title: s.socket }, s.socket)];
  if (!s.running) left.push(el("button", { className: "btn small", disabled: sb.busy || !s.server_bin, onclick: startServer,
    title: s.server_bin ? s.server_bin : "No se encontró `shepherd`: ponlo en el PATH o define SHEPHERD_BIN" }, "Iniciar servidor"));
  renderNotice();
  bar.replaceChildren(...left, el("span", { className: "grow" }), el("span", { className: "note" }, sb.note),
    el("button", { className: "mini", title: "Comandos y CLI", onclick: toggleDock }, "Comandos · Ctrl+Shift+P"));
}

// A row under the panels (not a pop-up) that says why tasks and role agents fail, and how to fix it.
function renderNotice() {
  const box = $("notice");
  box.hidden = !shepherdOff();
  if (box.hidden) return;
  if (!sb.enabling) {
    box.replaceChildren(el("span", {}, "La capa Shepherd está desactivada en este servidor: sin ella no se pueden crear tareas, lanzar agentes con rol ni ejecutar patrones."),
      el("button", { className: "btn small", onclick: async () => { sb.cfg = await invoke("shepherd_config").catch(() => null); sb.enabling = true; renderNotice(); } }, "Activar Shepherd…"));
    return;
  }
  box.replaceChildren(el("span", {}, `Se escribirá «[shepherd] enabled = true» en ${sb.cfg?.path ?? "config.toml"} (con copia .bak) y se REINICIARÁ el servidor: se cerrarán todos los terminales y agentes abiertos.`),
    el("button", { className: "btn small danger", disabled: sb.busy, onclick: enableShepherdNow }, sb.busy ? "Reiniciando…" : "Activar y reiniciar"),
    el("button", { className: "mini", disabled: sb.busy, onclick: () => { sb.enabling = false; renderNotice(); } }, "Cancelar"));
}

async function enableShepherdNow() {
  sb.busy = true; renderNotice();
  try { sb.status = await invoke("enable_shepherd"); sb.enabling = false; flash("Shepherd activado"); }
  catch (e) { flash(String(e)); }
  sb.busy = false;
  await refreshStatus();
  if (typeof onServerConnected === "function") onServerConnected();
}

function initStatus() {
  refreshStatus();
  const listen = window.__TAURI__?.event?.listen;
  if (listen) listen("shepherd-connection", async () => { await refreshStatus(); if (sb.status?.running && typeof onServerConnected === "function") onServerConnected(); });
}
