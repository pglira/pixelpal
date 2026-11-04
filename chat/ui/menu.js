// The menu of the pal. Each button sends its action to the backend, which
// hides the menu.

const { invoke } = window.__TAURI__.core;

for (const b of document.querySelectorAll("button")) {
  b.onclick = () => invoke("menu_action", { action: b.dataset.action });
}
document.addEventListener("keydown", (ev) => {
  if (ev.key === "Escape") invoke("menu_action", { action: "close" });
});
const { listen } = window.__TAURI__.event;

/// Shows the action of the first button for the state of the chat panel.
function update(panelOpen) {
  document.getElementById("chat").textContent = panelOpen ? "Hide chat" : "Open chat";
}

listen("menu", (ev) => update(ev.payload));
invoke("menu_view").then(update);
