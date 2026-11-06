// The settings window. The page edits a copy of the configuration; Apply and
// OK send the copy to the backend, which checks and stores it.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const form = document.getElementById("form");
const statusLine = document.getElementById("status");

let view = null;
let cfg = null;
/// Functions that update the enabled state and the visibility of fields.
let refreshers = [];
/// Functions that set the fields from `cfg` again.
let loaders = [];

const get = (path) => path.split(".").reduce((o, k) => o[k], cfg);
function set(path, value) {
  const keys = path.split(".");
  const last = keys.pop();
  keys.reduce((o, k) => o[k], cfg)[last] = value;
  refresh();
}

function refresh() {
  for (const f of refreshers) f();
}

function node(tag, props = {}, ...children) {
  const n = document.createElement(tag);
  Object.assign(n, props);
  n.append(...children);
  return n;
}

const hex = (c) => "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("");
const rgb = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const percent = (v) => Math.round(v * 100) + " %";

// ---- Field builders. Each returns a node and registers its loader. ----

function check(path) {
  const n = node("input", { type: "checkbox" });
  n.onchange = () => set(path, n.checked);
  loaders.push(() => (n.checked = get(path)));
  return n;
}

function number(path, { min, max, step = 1, unit = "", int = false, enabled }) {
  const n = node("input", { type: "number", min, max, step });
  n.onchange = () => {
    let v = int ? parseInt(n.value, 10) : parseFloat(n.value);
    if (!Number.isFinite(v)) v = get(path);
    v = Math.min(max, Math.max(min, v));
    n.value = v;
    set(path, v);
  };
  loaders.push(() => (n.value = get(path)));
  if (enabled) refreshers.push(() => (n.disabled = !enabled()));
  return unit ? node("span", { className: "field" }, n, node("span", { className: "unit", textContent: unit })) : n;
}

function slider(path, { min, max, step = 0.01, enabled }) {
  const n = node("input", { type: "range", min, max, step });
  const label = node("span", { className: "unit" });
  n.oninput = () => {
    label.textContent = percent(parseFloat(n.value));
    set(path, parseFloat(n.value));
  };
  loaders.push(() => {
    n.value = get(path);
    label.textContent = percent(get(path));
  });
  if (enabled) refreshers.push(() => (n.disabled = !enabled()));
  return node("span", { className: "field" }, n, label);
}

function text(path, { placeholder = "", wide = false } = {}) {
  const n = node("input", { type: "text", placeholder, className: wide ? "wide" : "" });
  n.oninput = () => set(path, n.value);
  loaders.push(() => (n.value = get(path)));
  return n;
}

function radio(path, options) {
  const name = "r-" + path;
  const field = node("span", { className: "field" });
  for (const [value, label] of options) {
    const n = node("input", { type: "radio", name });
    n.onchange = () => n.checked && set(path, value);
    loaders.push(() => (n.checked = get(path) === value));
    field.append(node("label", {}, n, " " + label));
  }
  return field;
}

/// A color field, and a button that picks the color of a pixel on the screen.
function color(path) {
  const n = node("input", { type: "color" });
  n.oninput = () => set(path, rgb(n.value));
  loaders.push(() => (n.value = hex(get(path))));
  const pickButton = node("button", { className: "small eyedropper", title: "Pick a color from the screen: click a pixel, or right-click to cancel" });
  pickButton.innerHTML =
    '<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M11.6 1.2a1.9 1.9 0 0 1 2.7 2.7l-1.9 1.9.7.7-1.1 1.1-.7-.7-5.6 5.6H3.6l-1.3 1.3-1.1-1.1 1.3-1.3v-2.1l5.6-5.6-.7-.7 1.1-1.1.7.7 1.9-1.9zM4.2 10.6v1.2h1.2l5.4-5.4-1.2-1.2-5.4 5.4z"/></svg>';
  pickButton.onclick = async () => {
    pickButton.disabled = true;
    statusLine.textContent = "";
    try {
      const c = await invoke("pick_screen_color");
      if (c) {
        set(path, c);
        n.value = hex(c);
      }
    } catch (e) {
      statusLine.textContent = String(e);
    } finally {
      pickButton.disabled = false;
    }
  };
  return node("span", { className: "field" }, n, pickButton);
}

/// A text field for a path with a Browse button that opens a dialog.
function pathField(path, kind, placeholder, clearable) {
  const t = text(path, { placeholder });
  const browse = node("button", { textContent: "Browse…" });
  browse.onclick = async () => {
    browse.disabled = true;
    try {
      const picked = await invoke("pick", { kind, start: get(path) });
      if (picked) {
        set(path, picked);
        load();
      }
    } catch (e) {
      statusLine.textContent = String(e);
    }
    browse.disabled = false;
  };
  const field = node("span", { className: "field" }, t, browse);
  if (clearable) {
    const x = node("button", { className: "small", textContent: "×", title: "Default font" });
    x.onclick = () => {
      set(path, "");
      load();
    };
    refreshers.push(() => (x.hidden = !get(path)));
    field.append(x);
  }
  return field;
}

function row(grid, label, tip, ...fields) {
  const l = node("label", { textContent: label });
  if (tip) l.title = tip;
  grid.append(l, node("span", { className: "field" }, ...fields));
}

function grid() {
  return node("div", { className: "grid" });
}

// ---- The form ----

function movement() {
  const g = grid();
  row(g, "Walk on the desktop", "", check("walk"));
  row(g, "Play actions", "Sword, spin attack, swimming, items and more from the game, between the walks.", check("actions"));
  row(g, "Action chance", "Probability that an action follows a pause instead of a walk.", slider("action_chance", { min: 0, max: 1, enabled: () => cfg.actions }));
  row(g, "Walk speed", "", number("walk_speed", { min: 5, max: 400, unit: "px/s" }));
  row(g, "Vary the walk speed", "Each walk is a stroll (½×), a walk (1×), a hurry (1.7×) or a dash with the Pegasus Shoes (3.5×).", check("vary_speed"));
  row(g, "Hop while walking", "Link hops at random moments of a walk. His shadow stays on the ground.", check("hops"));
  row(g, "Dash to the screen edge", "Now and then, Link dashes with the Pegasus Shoes to the edge of the screen and bounces back from it, as from a wall in the game. This value is the probability that a walk is such a dash.", check("edge_dash"), slider("edge_dash_chance", { min: 0, max: 1, enabled: () => cfg.edge_dash }));
  row(g, "React to Claude's tools", "Link reads for Read, digs for Edit, hammers for Bash, catches for WebSearch, and gets a shock when a tool fails.", check("tool_reactions"));
  row(g, "Chase the mouse pointer", "When the pointer comes near the ground beside Link, he runs to it and catches it with the bug net.", check("chase"));
  row(g, "Fight enemies", "Octoroks, soldiers and Buzzblobs appear now and then, Keese at night. Link fights them with his sword. This value is the mean time between two enemies.", check("enemies"), number("enemy_minutes", { min: 0.5, max: 240, step: 0.1, unit: "min", enabled: () => cfg.enemies }));
  row(g, "Day and night", "At night, Link is darker, strolls, and only dozes, sleeps, reads or prays. Equal hours mean a night that lasts all day.", check("day_night"), "night from", number("night_start", { min: 0, max: 23, int: true, unit: ":00", enabled: () => cfg.day_night }), "to", number("night_end", { min: 0, max: 23, int: true, unit: ":00", enabled: () => cfg.day_night }));
  row(g, "Sleep when you are away", "Link sleeps after this time without keyboard or mouse input, and wakes up when you return.", check("idle_sleep"), number("idle_minutes", { min: 0.5, max: 240, step: 0.1, unit: "min", enabled: () => cfg.idle_sleep }));
  row(g, "Pause between walks", "", number("pause_min", { min: 0, max: 600, step: 0.2, unit: "s" }), "to", number("pause_max", { min: 0, max: 600, step: 0.2, unit: "s" }));
  row(g, "Rest after a chat", "Link stays in place for this time after the chat panel closes or Claude answers.", number("chat_rest", { min: 0, max: 3600, unit: "s" }));
  row(g, "Scale", "", radio("scale", [[2, "2×"], [3, "3×"], [4, "4×"]]));
  row(g, "Walk distance per frame", "Larger values give slower steps for the same walk speed.", number("walk_stride", { min: 0.5, max: 10, step: 0.05, unit: "px" }));
  row(g, "Gravity", "Acceleration of a fall after a drag. 9.81 m/s² is the gravity of the earth.", number("gravity", { min: 0.5, max: 100, step: 0.1, unit: "m/s²" }));
  row(g, "Bottom margin", "", number("bottom_margin", { min: 0, max: 500, unit: "px" }));
  row(g, "Gear grows with chats", "Completed chats since the start of pixelpal unlock the Master Sword, the Blue Mail, the Tempered Sword, and the Golden Sword with the Red Mail.", check("gear_progress"), node("span", { className: "unit", textContent: view.progress }));
  return [node("h2", { textContent: "Movement" }), g];
}

function permissionNote() {
  const note = node("div", { className: "note" });
  refreshers.push(() => {
    const notes = {
      chat_only: "Claude has no tools. It only writes text.",
      read_only: "Claude can use: " + view.read_only_tools.join(", ") + ".",
      edit_files: "Claude can change files in the working directory without a question.",
      everything: "Warning: Claude can change files and run commands without a question.",
      custom: "Claude can use the listed tools without a question. Other tools that need a permission fail.",
    };
    note.textContent = notes[cfg.permission] || "";
    note.className = cfg.permission === "everything" ? "warn" : "note";
  });
  return note;
}

function modelList() {
  const list = node("div", { className: "models" });
  const draw = () => {
    list.replaceChildren(
      ...cfg.models.map((m, i) => {
        const r = node("div", {}, node("span", { textContent: m }));
        if (cfg.models.length > 1) {
          const x = node("button", { className: "small", textContent: "×", title: "Remove" });
          x.onclick = () => {
            cfg.models.splice(i, 1);
            if (!cfg.models.includes(cfg.model)) cfg.model = cfg.models[0];
            draw();
          };
          r.append(x);
        }
        return r;
      }),
    );
  };
  loaders.push(draw);
  const input = node("input", { type: "text", placeholder: "claude-opus-5-5" });
  const add = node("button", { textContent: "Add" });
  add.onclick = () => {
    const name = input.value.trim();
    if (name && !cfg.models.includes(name)) {
      cfg.models.push(name);
      input.value = "";
      draw();
    }
  };
  return node("div", { className: "block" }, node("label", { textContent: "Models in the chat panel" }), list, node("span", { className: "field" }, input, add));
}

function claude() {
  const g = grid();
  row(g, "claude binary", "", text("claude_path"));
  row(g, "Working directory", "", pathField("working_dir", "folder", "home directory", false));
  const perm = node("select");
  for (const [value, label] of view.permissions) perm.append(new Option(label, value));
  perm.onchange = () => set("permission", perm.value);
  loaders.push(() => (perm.value = cfg.permission));
  row(g, "Permissions", "", perm);
  const toolsLabel = node("label", { textContent: "Allowed tools" });
  const tools = node("span", { className: "field" }, text("custom_tools", { placeholder: "Read Grep Bash(git log:*)" }));
  refreshers.push(() => (toolsLabel.hidden = tools.hidden = cfg.permission !== "custom"));
  g.append(toolsLabel, tools);
  const instructions = node("textarea", { rows: 4, placeholder: "Answer in short sentences." });
  instructions.oninput = () => set("instructions", instructions.value);
  loaders.push(() => (instructions.value = cfg.instructions));
  return [
    node("h2", { textContent: "Claude" }),
    g,
    permissionNote(),
    node(
      "div",
      { className: "block" },
      node("label", {
        textContent: "Instructions",
        title: "Claude reads these instructions in each chat, in addition to its system prompt. A change applies to the next message.",
      }),
      instructions,
    ),
    modelList(),
  ];
}

function panel() {
  const size = grid();
  row(size, "Width", "", number("panel_width", { min: 280, max: 1600, unit: "px" }));
  row(size, "Height", "", number("panel_height", { min: 240, max: 1400, unit: "px" }));
  row(
    size,
    "Bottom gap",
    "The space between the bottom of the screen and the bottom of the chat panel.",
    number("panel_gap", { min: 0, max: 4000, unit: "px" }),
  );
  row(
    size,
    "Wrap code lines",
    "Long lines in code blocks continue on the next line. Without the wrap, a code block scrolls to the side.",
    check("code_wrap"),
  );
  const t = grid();
  const preset = (name, theme) => {
    const b = node("button", { textContent: name });
    b.onclick = () => {
      const keep = { font_file: cfg.theme.font_file, font_size: cfg.theme.font_size, opacity: cfg.theme.opacity };
      cfg.theme = { ...structuredClone(theme), ...keep };
      load();
    };
    return b;
  };
  row(t, "Preset", "", preset("Dark", view.dark_theme), preset("Light", view.light_theme));
  row(t, "Widget style", "", radio("theme.dark", [[true, "Dark"], [false, "Light"]]));
  for (const [label, key] of [
    ["Background", "background"],
    ["Text", "text"],
    ["Input background", "input_background"],
    ["Your messages", "user_message"],
    ["Accent", "accent"],
  ]) {
    row(t, label, "", color("theme." + key));
  }
  row(t, "Background opacity", "", slider("theme.opacity", { min: 0.1, max: 1 }));
  row(t, "Font size", "", number("theme.font_size", { min: 8, max: 40, step: 0.2, unit: "px" }));
  row(t, "Font file", "", pathField("theme.font_file", "font", "default font", true));
  const fontError = node("div", { className: "error" });
  loaders.push(() => (fontError.textContent = view.font_error || ""));
  return [node("h2", { textContent: "Chat panel" }), size, node("h3", { textContent: "Theme" }), t, fontError];
}

function general() {
  const g = grid();
  row(g, "Start pixelpal at login", "", check("autostart"));
  row(g, "Chat shortcut", "A key with one or more modifiers: Ctrl, Alt, Shift, Super. The shortcut works in all programs and opens or closes the chat.", text("chat_shortcut", { placeholder: "Super+Shift+C" }));
  row(g, "Pal shortcut", "A key with one or more modifiers: Ctrl, Alt, Shift, Super. The shortcut works in all programs and hides or shows the pal.", text("pal_shortcut", { placeholder: "Super+Shift+H" }));
  const errors = node("div", { className: "error", id: "key-errors" });
  const path = node("div", { className: "path" });
  loaders.push(() => {
    const lines = [];
    if (view.chat_key_error) lines.push("Chat shortcut: " + view.chat_key_error);
    if (view.pal_key_error) lines.push("Pal shortcut: " + view.pal_key_error);
    errors.textContent = lines.join("\n");
    errors.style.whiteSpace = "pre-wrap";
    path.textContent = "Settings file: " + view.config_path;
  });
  return [node("h2", { textContent: "General" }), g, errors, path];
}

function build() {
  refreshers = [];
  loaders = [];
  form.replaceChildren(...movement(), ...claude(), ...panel(), ...general());
  load();
}

/// Sets all fields from `cfg` and `view`.
function load() {
  for (const f of loaders) f();
  refresh();
}

function show(v) {
  view = v;
  cfg = structuredClone(v.config);
  const top = form.scrollTop;
  build();
  form.scrollTop = top;
  statusLine.textContent = v.status || "";
}

async function apply() {
  try {
    show(await invoke("apply_settings", { config: cfg }));
    return true;
  } catch (e) {
    statusLine.textContent = String(e);
    return false;
  }
}

document.getElementById("ok").onclick = async () => {
  if (await apply()) invoke("close_settings");
};
document.getElementById("apply").onclick = apply;
document.getElementById("cancel").onclick = () => invoke("close_settings");
document.addEventListener("keydown", (ev) => {
  if (ev.key === "Escape") invoke("close_settings");
});

// The shortcut errors come from the pal process.
listen("settings", (ev) => {
  view.chat_key_error = ev.payload.chat_key_error;
  view.pal_key_error = ev.payload.pal_key_error;
  load();
});
// A resize drag of the chat panel changes its size.
listen("panel_size", (ev) => {
  [cfg.panel_width, cfg.panel_height] = ev.payload;
  load();
});

invoke("get_settings").then(show);
