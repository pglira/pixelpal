# pixelpal

A desktop pal for Linux. Link from *The Legend of Zelda: A Link to the Past* walks along the bottom of the screen. A click on him opens a chat with Claude through the `claude` CLI (Claude Code).

- Link walks, hops, dashes with the Pegasus Shoes, and plays actions from the game: sword swings, the spin attack, items, medallions, rocks, bushes, and more.
- Each tool call of Claude gets an item of the game. A failed tool gives a shock.
- Completed chats unlock better gear, up to the Golden Sword and the Red Mail.
- At night, Link is calm. When you are away, he sleeps. He chases the mouse pointer.

## Requirements

- Linux with X11 (or XWayland), a compositor, and a StatusNotifierItem tray.
- Rust 1.95 or newer, and Claude Code (`claude`), logged in.
- Your ROM of *A Link to the Past* (USA), SHA-1 `6d4f10a8b10e10dbe624cb23cf03b88bb8252973`.
- Optional: `zenity` or `kdialog`, for the file dialogs.

## Install

```sh
./install.sh [rom-file]
```

The script installs `~/.local/bin/pixelpal`, an icon and a menu entry. `./install.sh --uninstall` removes them.

## The ROM

The graphics belong to Nintendo, so pixelpal does not contain them. At the first start, pixelpal asks for the ROM file, checks its SHA-1, and stores a copy in `~/.local/share/pixelpal/`. Then it draws Link from the ROM. Without a dialog program, use `pixelpal --import-rom <file>`.

`assets/link.json` tells pixelpal where each tile and color is in the ROM. It holds no graphics. `tools/link-extract` makes it with [zelda3](https://github.com/snesrev/zelda3).

## Use

- **Left click:** open or close the chat. **Drag:** move Link.
- **Middle click:** poke Link.
- **Right click** or the tray icon: menu with the settings.

The settings are in `~/.config/pixelpal/config.toml`. Because `claude -p` cannot ask for permissions, a preset in the settings selects the tools of Claude: chat only, read-only, edit files, everything, or a custom list.

## Development

- `PIXELPAL_ROM=<file> cargo test --release` runs the tests. Without `PIXELPAL_ROM`, the tests use the stored ROM.
- `PIXELPAL_ACTION=<name> cargo run --release` plays one action again and again. The names are in `src/actions.rs`.

## Known limits

- With HiDPI X11, the first position can be on the wrong monitor.
- **Stop** cannot end a `claude` process that closes its output but does not exit.
