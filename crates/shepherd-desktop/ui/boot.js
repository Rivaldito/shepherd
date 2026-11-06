// First script: reads how this window was opened. `?view=monitor&root=…&focus=…` is the monitor window,
// which shows only the monitor, for the project `root`.
window.__view = Object.fromEntries(new URLSearchParams(location.search));
if (window.__view.view === "monitor") document.body.classList.add("monitor-window");
