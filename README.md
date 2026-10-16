# pixelpal

A desktop pal for Linux. Link from *The Legend of Zelda: A Link to the Past* walks along the bottom edge of the screen. A click on him opens a chat with Claude. The chat uses the `claude` CLI (Claude Code) in the background.

- The pal is always on top of the other windows and on all desktops. It has no taskbar entry.
- Left click: open or close the chat panel. Left drag: move the pal (not while the chat panel is open). When you release him, he falls back to the bottom edge.
- Middle click: poke Link. He reacts at once with an electric shock, a spin attack, a swoon or sword swings (`REACTIONS` in `src/actions.rs`).
- Right click: menu with **Open chat**, **Settings…** and **Quit**.
- Tray icon: a click opens or closes the chat. Its menu has **Hide pal** / **Show pal**, **Open or close the chat**, **Settings…** and **Quit**.
- Between the walks, the pal plays random actions from the game: sword swings, the spin attack, swimming, the bow, the hammer, the bug net, the medallions, a fall into a pit (after which he drops from the top of the screen), sleep, and more. The list is `ACTIONS` in `src/actions.rs`.
- Link also finds rocks, skull pots and bushes: he lifts, carries and throws a rock or a skull, which breaks, and he cuts a bush with his sword.
- While Claude works, the pal shows a "…" bubble, and each tool call of Claude gets an item of the game: he reads for `Read`/`Grep`/`Glob`, digs for `Edit`, uses the Cane of Somaria for `Write`, swings the hammer for `Bash`, the bug net for `WebSearch`, the hookshot for `WebFetch`, and the magic powder for `Task`. A failed tool gives him a shock. When the reply is complete, he holds up his arms.
- Completed chats unlock better gear: the Master Sword (10 chats), the Blue Mail (30), the Tempered Sword and the Mirror Shield (75), the Golden Sword and the Red Mail (150). A new tier comes with a level up: Link pulls the Master Sword and holds it up. The count is in `~/.local/share/pixelpal/progress.json`.
- At night (22:00 to 7:00 by default), Link is darker, strolls, and only dozes, sleeps, reads or prays.
- When the mouse pointer comes near the ground beside Link, he runs to it and catches it with the bug net. After a catch, he waits 30 s before the next chase.
- When you are away (no keyboard or mouse input for 5 minutes by default), Link sleeps with Zzz. When you return, he wakes up.
- Only Link's body takes clicks. Clicks on the rest of the pal window, which is empty space for the swords, items and objects of the actions, go to the windows below.

## Requirements

- Linux with X11. On a Wayland session, pixelpal runs through XWayland.
- A panel with a StatusNotifierItem tray, for the tray icon (on XFCE: the "Status Tray" plugin).
- A compositor, for the transparent window. On XFCE: Settings → Window Manager Tweaks → Compositor → Enable display compositing.
- Rust 1.95 or newer.
- The ROM of *A Link to the Past* (USA). See [The ROM](#the-rom).
- Optional: `zenity` or `kdialog`, for the ROM dialog at the first start and for the **Browse…** buttons of the settings. Without them, use `pixelpal --import-rom <file>` and type the paths.
- Claude Code (`claude`), logged in.

## The ROM

The graphics of Link belong to Nintendo. pixelpal does not contain them: it draws them at start from your own ROM of *The Legend of Zelda: A Link to the Past* (USA). The ROM must have this SHA-1 (without a copier header):

```
6d4f10a8b10e10dbe624cb23cf03b88bb8252973
```

At the first start, pixelpal opens a file dialog for the ROM. It checks the size and the SHA-1, removes a copier header of 512 bytes, and stores a copy as `~/.local/share/pixelpal/zelda3.sfc`. If the check fails, the dialog opens again. Without `zenity` or `kdialog`, import the ROM on the command line:

```sh
pixelpal --import-rom <file>
```

`assets/link.json` is the sprite recipe. It holds no graphics: for each 8×8 tile of each pose it names the place of the pixels and the colors in the ROM, and the position of the tile. It also holds the frame sizes and the playback sequences. `tools/link-extract` makes the recipe with the drawing code of [zelda3](https://github.com/snesrev/zelda3); see `tools/link-extract/README.md`. You need zelda3 only to change the recipe.

The tests draw the strips too. They use the stored ROM, or the ROM file in `PIXELPAL_ROM`: `PIXELPAL_ROM=<file> cargo test --release`.

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

`PREFIX` changes the directory of the binary, and `XDG_DATA_HOME` the directory of the icon and the desktop file. `./install.sh --uninstall` removes these files. It keeps the settings, the last chat and the stored ROM.

To import the ROM during the installation, give its path: `./install.sh <rom-file>`. Otherwise, the script asks for the ROM when it writes the icon.

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
| Hop while walking | Lets Link hop at random moments of a walk (about one hop in three seconds). The shadow of the game stays on the ground. The height follows the gravity setting. |
| React to Claude's tools | Link uses an item for each tool call of Claude. |
| Chase the mouse pointer | Link chases the pointer when it comes near the ground beside him. |
| Day and night, night hours | Calm behavior and a darker Link during the night hours (local time). |
| Sleep when you are away, minutes | Link sleeps after this time without keyboard or mouse input (XScreenSaver extension). |
| Gear grows with chats | Completed chats unlock better gear. Off: Link keeps the Fighter's Sword. The window shows the count and the next tier. |
| Play actions, action chance | Turns the random actions on or off, and sets the probability that an action follows a pause instead of a walk (default 50 %). |
| Walk speed, pause between walks | Speed in px/s, and the random pause in seconds. |
| Vary the walk speed | Each walk gets a random gait: a stroll (½ of the walk speed), a walk, a hurry (1.7×), or a dash with the Pegasus Shoes (3.5×): a short run in place, then a run with the sword held forward. |
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

## Resource use

pixelpal repaints only when Link changes, and it polls X11 at most once per second when he stands still. Measured on X11 with Mesa at scale 3:

| State | CPU | Memory (PSS) |
|---|---|---|
| Standing | about 0.1 % | about 50 MB, mostly shared GL libraries |
| Walking and actions | about 0.8 % | the same |
| Asleep while you are away | about 0.8 %, and 1 frame per second after 30 minutes | the same |

## Known limits

- On HiDPI X11 setups (`Xft.dpi` above 96), the first position at start can be on the wrong monitor. The pal moves to the correct place after the first fall.
- If `claude` closes its output but does not exit, **Stop** cannot end it.

## Source layout

| File | Content |
|---|---|
| `src/main.rs` | Start position and window setup |
| `src/app.rs` | The egui application: pal window, chat panel, menu, settings window |
| `src/pal.rs` | Walking, falling, actions, animation states, speech bubble and stars |
| `src/actions.rs` | The actions and reactions, built from the playback sequences of the game, and the object scenes |
| `src/link.rs` | The strips of one gear tier, drawn from the ROM with the sprite recipe `assets/link.json` |
| `src/rom.rs` | Check, import and file dialog of the ROM |
| `src/progress.rs` | Completed chats and the gear tiers they unlock |
| `src/picker.rs` | File and folder dialogs through `zenity` or `kdialog` |
| `tools/link-extract/` | Makes the sprite recipe with the drawing code of zelda3 |
| `src/workarea.rs` | Usable screen area, pointer position and window states from X11 |
| `src/theme.rs` | Colors and font of the chat panel |
| `src/tray.rs` | Tray icon (StatusNotifierItem) and its menu |
| `src/chat.rs` | Chat panel and conversation |
| `src/claude.rs` | `claude` child process and stream parser |
| `src/settings.rs` | Configuration file and settings window |
| `src/history.rs` | Stored last chat |
