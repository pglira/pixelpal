// The menu of the pal. Each button sends its action to the backend, which
// closes the menu.

const { invoke } = window.__TAURI__.core;

for (const b of document.querySelectorAll("button")) {
  b.onclick = () => invoke("menu_action", { action: b.dataset.action });
}
document.addEventListener("keydown", (ev) => {
  if (ev.key === "Escape") invoke("menu_action", { action: "close" });
});
invoke("menu_view").then((panelOpen) => {
  document.getElementById("chat").textContent = panelOpen ? "Hide chat" : "Open chat";
});
