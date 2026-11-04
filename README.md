# pixelpal

A desktop pal for Linux. Link from *The Legend of Zelda: A Link to the Past* walks along the bottom of the screen. A click on him opens a chat with Claude through the `claude` CLI (Claude Code).

- Link walks, hops, dashes with the Pegasus Shoes, and plays actions from the game: sword swings, the spin attack, items, medallions, rocks, bushes, and more.
- Each tool call of Claude gets an item of the game. A failed tool gives a shock.
- Completed chats unlock better gear, up to the Golden Sword and the Red Mail after 20 chats. Each start begins again with the first gear.
- Now and then an enemy appears: an Octorok, a soldier or a Buzzblob, and a Keese at night. Link fights it with his sword and picks up the heart or rupee that it drops.
- At night, Link is calm. When you are away, he sleeps. He chases the mouse pointer.

## Requirements

- Linux with X11 (or XWayland), a compositor, and a StatusNotifierItem tray.
- WebKitGTK 4.1 for the chat. To build: `libwebkit2gtk-4.1-dev` (Debian, Ubuntu) or `webkit2gtk4.1-devel` (Fedora).
- Rust 1.95 or newer, and Claude Code (`claude`), logged in.
- Your ROM of *A Link to the Past* (USA), SHA-1 `6d4f10a8b10e10dbe624cb23cf03b88bb8252973`.
- Optional: `zenity` or `kdialog`, for the file dialogs.

## Install

```sh
./install.sh [rom-file]
```

The script installs `~/.local/bin/pixelpal`, `~/.local/bin/pixelpal-chat`, an icon and a menu entry. `./install.sh --uninstall` removes them.

## The ROM

The graphics belong to Nintendo, so pixelpal does not contain them. At the first start, pixelpal asks for the ROM file, checks its SHA-1, and stores a copy in `~/.local/share/pixelpal/`. Then it draws Link from the ROM. Without a dialog program, use `pixelpal --import-rom <file>`.

`assets/link.json` tells pixelpal where each tile and color is in the ROM. It holds no graphics. `tools/link-extract` makes it with [zelda3](https://github.com/snesrev/zelda3).

## Use

- **Left click:** open or close the chat. **Drag:** move Link.
- **Middle click:** poke Link.
- **Right click** or the tray icon: menu with the settings.
- **Chat shortcut:** a global shortcut in the settings, such as `Super+Shift+C`, opens or closes the chat.
- **Pal shortcut:** a global shortcut in the settings, such as `Super+Shift+H`, hides or shows Link.
- **History** in the chat panel: the stored chats, with a search. A click opens a chat, and the next prompt continues it in its working directory.

The settings are in `~/.config/pixelpal/config.toml`. Because `claude -p` cannot ask for permissions, a preset in the settings selects the tools of Claude: chat only, read-only, edit files, everything, or a custom list. The **Instructions** field in the settings holds your own instructions: Claude gets them in each chat as an addition to its system prompt.

## Structure

pixelpal runs as two processes, so that the pal and the chat do not depend on the same UI framework:

- **`pixelpal`** (`pal/`): the pal and the enemies. It draws them with `tiny-skia` into X11 windows of its own (`x11rb`), and has the tray icon and the global shortcuts. It starts the chat process.
- **`pixelpal-chat`** (`chat/`): the chat panel, the menu and the settings window, as web pages in Tauri 2 windows (`chat/ui/`). It runs `claude`.
- **`pixelpal-core`** (`core/`): the configuration, the stored chats and images, the `claude` runner, and the messages between the two processes.

The two processes talk through the stdin and the stdout of the chat process, with one JSON message per line (`core/src/ipc.rs`). `pixelpal` looks for `pixelpal-chat` in its own directory, and then in `PATH`.

## Development

- `cargo build --release` builds both programs into `target/release/`.
- `PIXELPAL_ROM=<file> cargo test --release --workspace` runs the tests. Without `PIXELPAL_ROM`, the tests use the stored ROM.
- `PIXELPAL_ACTION=<name> cargo run --release -p pixelpal` plays one action again and again. The names are in `pal/src/actions.rs`.
- `PIXELPAL_ENEMY=<name> cargo run --release -p pixelpal` sends one kind of enemy every few seconds: `octorok`, `green_soldier`, `blue_soldier`, `buzzblob` or `keese`.

## Known limits

- pixelpal uses the physical pixels of the screen. On a HiDPI screen, use a larger **Scale** in the settings.
- **Stop** cannot end a `claude` process that closes its output but does not exit.
