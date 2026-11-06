// Bottom status bar: connection to the Shepherd server (Herdr shows machine · branch · agents here).
// Starting a server is an explicit button: it creates real state, so the app never does it by itself.
let sb = { status: null, note: "", busy: false };

async function refreshStatus() {
  try { sb.status = await invoke("api_status"); }
  catch (e) { sb.status = { running: false, socket: "", server_bin: null }; sb.note = String(e); }
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
  bar.replaceChildren(...left, el("span", { className: "grow" }), el("span", { className: "note" }, sb.note));
}

function initStatus() {
  refreshStatus();
  const listen = window.__TAURI__?.event?.listen;
  if (listen) listen("shepherd-connection", async () => { await refreshStatus(); if (sb.status?.running && typeof onServerConnected === "function") onServerConnected(); });
}
