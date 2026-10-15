# pixelpal

A desktop pal for Linux. Link from *The Legend of Zelda: A Link to the Past* walks along the bottom edge of the screen. A click on him opens a chat with Claude. The chat uses the `claude` CLI (Claude Code) in the background.

- The pal is always on top of the other windows and on all desktops. It has no taskbar entry.
- Left click: open or close the chat panel. Left drag: move the pal (not while the chat panel is open). When you release him, he falls back to the bottom edge.
- Middle click: poke Link. He reacts at once with an electric shock, a spin attack, a swoon or sword swings (`REACTIONS` in `src/actions.rs`).
- Right click: menu with **Open chat**, **Settings…** and **Quit**.
- Tray icon: a click opens or closes the chat. Its menu has **Hide pal** / **Show pal**, **Open or close the chat**, **Settings…** and **Quit**.
- Between the walks, the pal plays random actions from the game: sword swings, the spin attack, swimming, the bow, the hammer, the bug net, the medallions, a fall into a pit (after which he drops from the top of the screen), sleep, and more. The list is `ACTIONS` in `src/actions.rs`.
- While Claude works, the pal shows a "…" bubble. When the reply is complete, he holds up his arms.
- Only Link's body reacts to the mouse. The rest of the pal window is empty space for the swords and items of the actions.

## Requirements

- Linux with X11. On a Wayland session, pixelpal runs through XWayland.
- A panel with a StatusNotifierItem tray, for the tray icon (on XFCE: the "Status Tray" plugin).
- A compositor, for the transparent window. On XFCE: Settings → Window Manager Tweaks → Compositor → Enable display compositing.
- Rust 1.95 or newer.
- GTK 3 development files, for the folder dialog (Debian/Ubuntu: `libgtk-3-dev`).
- Claude Code (`claude`), logged in.

## Animations

The graphics of Link belong to Nintendo. The repository does not contain them, and `.gitignore` excludes `assets/link/`. You extract them from your own copy of the game with `tools/link-extract`:

1. Build [zelda3](https://github.com/snesrev/zelda3) with your ROM, as its README describes. This gives `zelda3_assets.dat` and the object files.
2. Extract the animations into `assets/link/`:

   ```sh
   tools/link-extract/build.sh <zelda3-dir> /tmp/link-extract
   (cd <zelda3-dir> && /tmp/link-extract /tmp/poses.bin)
   python3 tools/link-extract/pack.py /tmp/poses.bin assets/link
   ```

See `tools/link-extract/README.md` for the details and the output format. The build of pixelpal stops with a message if `assets/link/animations.json` is missing. The binary embeds the animations, so the installed binary needs no other files.

To see one action again and again, start pixelpal with `PIXELPAL_ACTION=<name>`, for example `PIXELPAL_ACTION=spin_attack cargo run --release`. The names are in `ACTIONS` in `src/actions.rs`.

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

- Drag the top edge, a side edge or a top corner of the panel to resize it. The panel stays centered above the pal. The new size is saved at once.
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
| Play actions, action chance | Turns the random actions on or off, and sets the probability that an action follows a pause instead of a walk (default 50 %). |
| Walk speed, pause between walks | Speed in px/s, and the random pause in seconds. |
| Vary the walk speed | Each walk gets a random gait: a stroll (½ of the walk speed), a walk, a hurry (1.7×), or a Pegasus dash (3.5×) after a short run in place. |
| Walk distance per frame | Distance in sprite pixels for one frame of the walk cycle. Larger values give slower steps at the same walk speed. |
| Scale | Size of the pal: 2×, 3× or 4×. |
| Gravity | Acceleration of a fall after a drag, in m/s² (default 9.81). 16 sprite pixels count as 1 m. |
| Bottom margin | Extra distance between the pal and the bottom of the usable screen area. |
| claude binary | Path of `claude`. Default: `claude` from `PATH`. |
| Working directory | Directory in which `claude` runs. Empty: the home directory. |
| Permissions | See below. |
| Models | Entries of the model selector. |
| Chat panel width and height | Size of the chat panel. A resize with the mouse changes these values too. |
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
| `src/pal.rs` | Walking, falling, actions, animation states, speech bubble and stars |
| `src/actions.rs` | The random actions, built from the playback sequences of the game |
| `src/link.rs` | Loading of the embedded animations (`assets/link/`) |
| `build.rs` | Embeds the files of `assets/link/` |
| `tools/link-extract/` | Extraction of the animations from the game |
| `src/workarea.rs` | Usable screen area, pointer position and window states from X11 |
| `src/theme.rs` | Colors and font of the chat panel |
| `src/tray.rs` | Tray icon (StatusNotifierItem) and its menu |
| `src/chat.rs` | Chat panel and conversation |
| `src/claude.rs` | `claude` child process and stream parser |
| `src/settings.rs` | Configuration file and settings window |
| `src/history.rs` | Stored last chat |
