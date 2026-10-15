#!/bin/sh
# Builds pixelpal and installs it for the current user:
#   $PREFIX/bin/pixelpal                                  the binary
#   $DATA_DIR/icons/hicolor/<size>/apps/pixelpal.png      the icon
#   $DATA_DIR/applications/pixelpal.desktop               the desktop file
#
# PREFIX defaults to ~/.local, DATA_DIR to $XDG_DATA_HOME or ~/.local/share.
#
# Usage: ./install.sh              build and install
#        ./install.sh --uninstall  remove the installed files
#
# The settings (~/.config/pixelpal) and the last chat (~/.local/share/pixelpal)
# are never removed.

set -eu

PREFIX=${PREFIX:-$HOME/.local}
DATA_DIR=${XDG_DATA_HOME:-$HOME/.local/share}
BIN=$PREFIX/bin/pixelpal
ICON_SIZE=96x96
ICON=$DATA_DIR/icons/hicolor/$ICON_SIZE/apps/pixelpal.png
DESKTOP=$DATA_DIR/applications/pixelpal.desktop

cd "$(dirname "$0")"

refresh_caches() {
    command -v update-desktop-database >/dev/null 2>&1 &&
        update-desktop-database "$DATA_DIR/applications" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 &&
        gtk-update-icon-cache -q -t "$DATA_DIR/icons/hicolor" 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -fv "$BIN" "$ICON" "$DESKTOP"
    refresh_caches
    echo "pixelpal is uninstalled. The autostart entry, if any, is in ~/.config/autostart/pixelpal.desktop."
    exit 0
fi

if [ ! -f assets/link_sheet.png ]; then
    echo "assets/link_sheet.png is missing. See README.md, section \"Sprite sheet\"." >&2
    exit 1
fi

cargo build --release

install -Dm755 target/release/pixelpal "$BIN"
mkdir -p "$(dirname "$ICON")"
"$BIN" --write-icon "$ICON"
mkdir -p "$(dirname "$DESKTOP")"
# Quote the path for the Exec key: escape " ` $ \ and double %. Then double
# every backslash, because the string escape rule applies before the quoting rule.
EXEC=$(printf '%s' "$BIN" | sed -e 's/[\\"`$]/\\&/g' -e 's/%/%%/g' -e 's/\\/\\\\/g')
cat > "$DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=pixelpal
GenericName=Desktop pal
Comment=A desktop pal that walks along the bottom of the screen and chats through Claude Code
Exec="$EXEC"
Icon=pixelpal
Terminal=false
Categories=Utility;
StartupNotify=false
EOF

refresh_caches

echo "Installed:"
echo "  $BIN"
echo "  $ICON"
echo "  $DESKTOP"
case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "Note: $PREFIX/bin is not in PATH." ;;
esac
