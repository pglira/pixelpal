// The chat panel. The backend owns the chat; this page shows it and sends
// the actions of the user to the backend.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const el = {
  model: $("model"),
  newChat: $("new-chat"),
  historyToggle: $("history-toggle"),
  settings: $("settings"),
  close: $("close"),
  scroll: $("scroll"),
  messages: $("messages"),
  history: $("history"),
  historyQuery: $("history-query"),
  historyList: $("history-list"),
  attachments: $("attachments"),
  attachError: $("attach-error"),
  input: $("input"),
  send: $("send"),
  stop: $("stop"),
  busy: $("busy"),
  dir: $("dir"),
};

let messages = [];
let state = { running: false, attachments: [], dir: "", model: "", models: [], chat_id: null };
let historyOpen = false;

/// True if the message list shows its end, so that new text keeps it there.
function atBottom() {
  const s = el.scroll;
  return s.scrollHeight - s.scrollTop - s.clientHeight < 40;
}

function scrollToBottom() {
  el.scroll.scrollTop = el.scroll.scrollHeight;
}

function messageNode(m) {
  const div = document.createElement("div");
  div.className = "msg " + m.role;
  if (m.role === "user") {
    if (m.html) {
      const text = document.createElement("div");
      text.className = "text";
      text.innerHTML = m.html;
      div.appendChild(text);
    }
    if (m.images.length) {
      const imgs = document.createElement("div");
      imgs.className = "images";
      for (const url of m.images) {
        const img = document.createElement("img");
        img.src = url;
        img.alt = "(missing image)";
        imgs.appendChild(img);
      }
      div.appendChild(imgs);
    }
  } else {
    // The backend makes the HTML: escaped text, or safe Markdown HTML.
    div.innerHTML = m.html;
    for (const pre of div.querySelectorAll("pre")) pre.appendChild(copyButton(pre));
  }
  return div;
}

/// A button that copies the code of the code block `pre` to the clipboard.
function copyButton(pre) {
  const b = document.createElement("button");
  b.className = "copy";
  b.textContent = "Copy";
  b.title = "Copy to the clipboard";
  b.onclick = async (ev) => {
    ev.stopPropagation();
    const code = pre.querySelector("code");
    try {
      await invoke("copy_text", { text: (code || pre).textContent });
      b.textContent = "Copied";
    } catch (e) {
      b.textContent = "Error";
      b.title = String(e);
    }
    setTimeout(() => {
      b.textContent = "Copy";
      b.title = "Copy to the clipboard";
    }, 1500);
  };
  return b;
}

function thinkingNode() {
  const div = document.createElement("div");
  div.className = "thinking";
  div.id = "thinking";
  div.innerHTML = '<span class="spinner"></span><span>Claude is thinking…</span>';
  return div;
}

function renderMessages() {
  const stick = atBottom();
  el.messages.replaceChildren();
  if (!messages.length) {
    const hint = document.createElement("div");
    hint.className = "weak";
    hint.textContent = "Ask me something.";
    el.messages.appendChild(hint);
  }
  for (const m of messages) el.messages.appendChild(messageNode(m));
  renderThinking();
  if (stick) scrollToBottom();
}

function renderThinking() {
  const old = $("thinking");
  const last = messages[messages.length - 1];
  const show = state.running && (!last || last.role !== "assistant");
  if (show && !old) el.messages.appendChild(thinkingNode());
  if (!show && old) old.remove();
}

function updateMessage(index, m) {
  const stick = atBottom();
  if (index === messages.length) {
    if (!messages.length) el.messages.replaceChildren();
    messages.push(m);
    const t = $("thinking");
    if (t) t.remove();
    el.messages.appendChild(messageNode(m));
  } else if (index < messages.length) {
    messages[index] = m;
    const node = el.messages.querySelectorAll(".msg")[index];
    if (node) node.replaceWith(messageNode(m));
  } else {
    return;
  }
  renderThinking();
  if (stick) scrollToBottom();
}

function renderState() {
  const models = state.models.join("\n");
  if (el.model.dataset.models !== models) {
    el.model.replaceChildren(...state.models.map((m) => new Option(m, m)));
    el.model.dataset.models = models;
  }
  el.model.value = state.model;
  el.send.hidden = state.running;
  el.stop.hidden = !state.running;
  el.busy.hidden = !state.running;
  el.dir.textContent = state.dir;
  el.dir.title = state.dir;
  el.attachments.replaceChildren(
    ...state.attachments.map((url, i) => {
      const t = document.createElement("div");
      t.className = "thumb";
      const img = document.createElement("img");
      img.src = url;
      const x = document.createElement("button");
      x.textContent = "×";
      x.title = "Remove";
      x.onclick = () => invoke("remove_attachment", { index: i });
      t.append(img, x);
      return t;
    }),
  );
  el.attachments.hidden = !state.attachments.length;
  updateSend();
  renderThinking();
  if (historyOpen) loadHistory();
}

function updateSend() {
  el.send.disabled = !el.input.value.trim() && !state.attachments.length;
}

function applyTheme(t) {
  const root = document.documentElement.style;
  root.setProperty("--bg", t.background);
  root.setProperty("--text", t.text);
  root.setProperty("--input-bg", t.input_background);
  root.setProperty("--user-bg", t.user_message);
  root.setProperty("--accent", t.accent);
  root.setProperty("--font-size", t.font_size + "px");
  root.setProperty("color-scheme", t.dark ? "dark" : "light");
  let face = $("font-face");
  if (!face) {
    face = document.createElement("style");
    face.id = "font-face";
    document.head.appendChild(face);
  }
  face.textContent = t.font_url
    ? `@font-face { font-family: "ChatFont"; src: url("${t.font_url}"); }`
    : "";
}

function showAttachError(text) {
  el.attachError.textContent = text || "";
  el.attachError.hidden = !text;
}

async function send() {
  if (state.running || el.send.disabled) return;
  const text = el.input.value;
  if (await invoke("send", { text })) {
    el.input.value = "";
    showAttachError("");
    setHistory(false);
    updateSend();
    scrollToBottom();
  }
  el.input.focus();
}

function setHistory(open) {
  historyOpen = open;
  el.historyToggle.classList.toggle("active", open);
  el.history.hidden = !open;
  el.messages.hidden = open;
  if (open) {
    el.historyQuery.value = "";
    loadHistory();
    el.historyQuery.focus();
  } else {
    scrollToBottom();
  }
}

async function loadHistory() {
  const items = await invoke("history", { query: el.historyQuery.value });
  if (!items.length) {
    const none = document.createElement("div");
    none.className = "weak";
    none.textContent = "No stored chats.";
    el.historyList.replaceChildren(none);
    return;
  }
  el.historyList.replaceChildren(
    ...items.map((c) => {
      const row = document.createElement("div");
      row.className = "chat-item";
      const open = document.createElement("button");
      open.className = "open" + (c.id === state.chat_id ? " current" : "");
      open.disabled = state.running;
      open.title = state.running ? "Stop the running prompt first" : "Open and continue this chat";
      const title = document.createElement("span");
      title.className = "title";
      title.textContent = c.title;
      const detail = document.createElement("span");
      detail.className = "detail weak small";
      detail.textContent = c.detail;
      open.append(title, detail);
      open.onclick = async () => {
        try {
          await invoke("open_chat", { id: c.id });
          setHistory(false);
          el.input.focus();
        } catch (e) {
          showAttachError(String(e));
        }
      };
      const del = document.createElement("button");
      del.className = "icon";
      del.textContent = "×";
      del.title = "Delete";
      del.onclick = async () => {
        await invoke("delete_chat", { id: c.id });
        loadHistory();
      };
      row.append(open, del);
      return row;
    }),
  );
}

/// Attaches the image of a paste. A paste without text asks the backend for
/// the image of the clipboard, because not every image format reaches the
/// page.
async function handlePaste(ev) {
  const data = ev.clipboardData;
  const file = data && [...data.items].find((i) => i.kind === "file" && i.type.startsWith("image/"));
  try {
    if (file) {
      ev.preventDefault();
      const bytes = new Uint8Array(await file.getAsFile().arrayBuffer());
      let bin = "";
      for (let i = 0; i < bytes.length; i += 0x8000) {
        bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
      }
      await invoke("attach_image", { data: btoa(bin) });
      showAttachError("");
    } else if (!data || !data.getData("text/plain")) {
      ev.preventDefault();
      if (await invoke("paste_image")) showAttachError("");
    }
  } catch (e) {
    showAttachError(String(e));
  }
}

// Resize drags at the edges. The backend changes the size of the window; the
// movement comes in screen coordinates, so that the moving window does not
// change it.
for (const grip of document.querySelectorAll(".grip")) {
  const edges = grip.dataset.edges.split(" ");
  let start = null;
  let frame = 0;
  grip.addEventListener("pointerdown", (ev) => {
    if (ev.button !== 0) return;
    ev.preventDefault();
    grip.setPointerCapture(ev.pointerId);
    start = { x: ev.screenX, y: ev.screenY };
    invoke("resize_begin");
  });
  grip.addEventListener("pointermove", (ev) => {
    if (!start) return;
    const dx = ev.screenX - start.x;
    const dy = ev.screenY - start.y;
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() =>
      invoke("resize_by", {
        dx,
        dy,
        left: edges.includes("left"),
        right: edges.includes("right"),
        top: edges.includes("top"),
      }),
    );
  });
  const end = () => {
    if (!start) return;
    start = null;
    cancelAnimationFrame(frame);
    invoke("resize_end");
  };
  grip.addEventListener("pointerup", end);
  grip.addEventListener("pointercancel", end);
}

el.model.onchange = () => invoke("set_model", { model: el.model.value });
el.newChat.onclick = () => {
  invoke("new_chat");
  setHistory(false);
  el.input.focus();
};
el.historyToggle.onclick = () => setHistory(!historyOpen);
el.historyQuery.oninput = () => loadHistory();
el.settings.onclick = () => invoke("open_settings");
el.close.onclick = () => invoke("close_panel");
el.send.onclick = send;
el.stop.onclick = () => invoke("stop");
el.input.oninput = updateSend;
el.input.addEventListener("keydown", (ev) => {
  if (ev.key === "Enter" && !ev.shiftKey && !ev.isComposing) {
    ev.preventDefault();
    send();
  }
});
document.addEventListener("paste", handlePaste);
document.addEventListener("keydown", (ev) => {
  if (ev.key === "Escape") {
    ev.preventDefault();
    invoke("close_panel");
  } else if (ev.ctrlKey && !ev.altKey && ev.key.toLowerCase() === "l") {
    ev.preventDefault();
    el.newChat.onclick();
  }
});
// Links open in the default browser, never in the panel.
document.addEventListener("click", (ev) => {
  const a = ev.target.closest && ev.target.closest("a");
  if (!a) return;
  ev.preventDefault();
  const url = a.getAttribute("href");
  if (url) invoke("open_url", { url });
});

/// Shows the whole chat: the messages, the state and the theme.
function reset(view) {
  messages = view.messages;
  state = view.state;
  applyTheme(view.theme);
  renderMessages();
  renderState();
  scrollToBottom();
}

listen("reset", (ev) => reset(ev.payload));
listen("message", (ev) => updateMessage(ev.payload.index, ev.payload.message));
listen("state", (ev) => {
  state = ev.payload;
  renderState();
});
listen("theme", (ev) => applyTheme(ev.payload));
listen("focus", () => {
  if (!historyOpen) el.input.focus();
});

invoke("chat_view").then(reset);
el.input.focus();
