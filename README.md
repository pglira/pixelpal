# pixelpal

A desktop pal for Linux. Link from *The Legend of Zelda: A Link to the Past* walks along the bottom edge of the screen. A click on him opens a chat with Claude. The chat uses the `claude` CLI (Claude Code) in the background.

- The pal is always on top of the other windows and on all desktops. It has no taskbar entry.
- Left click: open or close the chat panel. Left drag: move the pal (not while the chat panel is open). When you release him, he falls back to the bottom edge.
- Right click: menu with **Open chat**, **Settings…** and **Quit**.
- Tray icon: a click opens or closes the chat. Its menu has **Hide pal** / **Show pal**, **Open or close the chat**, **Settings…** and **Quit**.
- While Claude works, the pal shows a "…" bubble. When the reply is complete, he holds up his arms.

## Requirements

- Linux with X11. On a Wayland session, pixelpal runs through XWayland.
- A panel with a StatusNotifierItem tray, for the tray icon (on XFCE: the "Status Tray" plugin).
- A compositor, for the transparent window. On XFCE: Settings → Window Manager Tweaks → Compositor → Enable display compositing.
- Rust 1.95 or newer.
- GTK 3 development files, for the folder dialog (Debian/Ubuntu: `libgtk-3-dev`).
- Claude Code (`claude`), logged in.

## Sprite sheet

The sprites of Link belong to Nintendo. The repository does not contain them, and `.gitignore` excludes the sheet. Before you build pixelpal:

1. Download the sheet "Link" of *The Legend of Zelda: A Link to the Past* (SNES, Playable Characters) from The Spriters Resource. The file name is `SNES - The Legend of Zelda_ A Link to the Past - Playable Characters - Link.png` (688 × 365 px).
2. Copy it to `assets/link_sheet.png`:

   ```sh
   cp "SNES - The Legend of Zelda_ A Link to the Past - Playable Characters - Link.png" assets/link_sheet.png
   ```

The build stops with a message if the file is missing. The binary embeds the sheet, so the installed binary needs no other files. pixelpal supports only this sheet: the frame rectangles are constants in `src/link.rs`.

## Install

```sh
./install.sh
```

The script builds the release binary and installs, for the current user:

| File | Content |
|---|---|
| `~/.local/bin/pixelpal` | the binary |
| `~/.local/share/icons/hicolor/96x96/apps/pixelpal.png` | the icon |
| `~/.local/share/applications/pixelpal.desktop` | the desktop file (application menu entry) |

`PREFIX` changes the directory of the binary, and `XDG_DATA_HOME` the directory of the icon and the desktop file. `./install.sh --uninstall` removes these files. It keeps the settings and the last chat.

To run pixelpal without installation:

```sh
cargo run --release
```

To start pixelpal at login, enable **Start pixelpal at login** in the settings. This writes `~/.config/autostart/pixelpal.desktop` with the path of the running binary.

## Chat

Each prompt starts `claude -p` with `--output-format stream-json`. The reply streams into the panel. The first reply gives a session id. The next prompts use `--resume <session id>`, so the conversation keeps its context.

- **Enter** sends the prompt. **Shift+Enter** adds a new line. **Esc** closes the panel.
- **Stop** kills the running `claude` process.
- **New chat** starts a new conversation.
- The model selector applies to the next prompt. `default` uses the default model of Claude Code.

pixelpal stores the last chat in `~/.local/share/pixelpal/last_chat.json`. At start, it asks whether to continue that chat. Claude Code stores sessions for each working directory, so pixelpal asks only if the working directory is the same.

## Settings

The settings are in the text file `~/.config/pixelpal/config.toml` (TOML). The settings window edits a copy of them: **Apply** applies and saves the changes, **OK** also closes the window, **Cancel** or closing the window discards unapplied changes.

| Setting | Effect |
|---|---|
| Walk on the desktop | Turns the walking on or off. |
| Walk speed, pause between walks | Speed in px/s, and the random pause in seconds. |
| Walk distance per frame | Distance in sprite pixels for one frame of the walk cycle. Larger values give slower steps at the same walk speed. |
| Scale | Size of the pal: 2×, 3× or 4×. |
| Bottom margin | Extra distance between the pal and the bottom of the usable screen area. |
| claude binary | Path of `claude`. Default: `claude` from `PATH`. |
| Working directory | Directory in which `claude` runs. Empty: the home directory. |
| Permissions | See below. |
| Models | Entries of the model selector. |
| Chat panel width and height | Size of the chat panel. |
| Theme | Dark or light preset, colors (background, text, input background, your messages, accent), background opacity, font size, and a TTF/OTF font file. |

`claude -p` cannot ask for a permission interactively. The permission presets set the tools of Claude:

| Preset | Flags | Effect |
|---|---|---|
| Chat only | `--tools ""` | No tools. Only text. |
| Read-only | `--tools` and `--allowedTools` with `Read Grep Glob WebSearch WebFetch` | Claude can read files and search the web. |
| Edit files | `--permission-mode acceptEdits` | Claude can also change files in the working directory. |
| Everything | `--permission-mode bypassPermissions` | Claude can do everything without a question. |
| Custom | `--allowedTools <your list>` | Claude can use the listed tools without a question. |

## Source layout

| File | Content |
|---|---|
| `src/main.rs` | Start position and window setup |
| `src/app.rs` | The egui application: pal window, chat panel, menu, settings window |
| `src/pal.rs` | Walking, falling, animation states, speech bubble and stars |
| `src/link.rs` | Frame rectangles of the sheet and color keying |
| `src/workarea.rs` | Usable screen area, pointer position and window states from X11 |
| `src/theme.rs` | Colors and font of the chat panel |
| `src/tray.rs` | Tray icon (StatusNotifierItem) and its menu |
| `src/chat.rs` | Chat panel and conversation |
| `src/claude.rs` | `claude` child process and stream parser |
| `src/settings.rs` | Configuration file and settings window |
| `src/history.rs` | Stored last chat |
