#!/usr/bin/env bash
# Inject the phase-13 PX4 board template into PX4-Autopilot so a
# normal `make px4_renode-h743_default` works.
#
# PX4's Makefile uses `find -maxdepth 3 -mindepth 3 -name
# '*.px4board'` (no `-L`) to discover boards, so a symlink wouldn't
# be visited. We copy `tests/renode/px4-board/` to
# `$PX4_AUTOPILOT_DIR/boards/px4/renode-h743/`. Re-run after
# editing files in `tests/renode/px4-board/` to mirror them.
#
# To remove: `rm -rf $PX4_AUTOPILOT_DIR/boards/px4/renode-h743`.

set -euo pipefail

PX4="${PX4_AUTOPILOT_DIR:-$HOME/repos/PX4-Autopilot}"
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$HERE/px4-board"
DEST="$PX4/boards/px4/renode-h743"
SENTINEL=".px4-rs-renode-h743"

if [ ! -d "$PX4" ]; then
    echo "PX4_AUTOPILOT_DIR ($PX4) doesn't exist; export it or pass via env." >&2
    exit 1
fi
if [ ! -d "$SRC" ]; then
    echo "Source board dir missing at $SRC" >&2
    exit 1
fi

# Sentinel guard: never overwrite a directory that doesn't carry
# our marker. Symlinks (from the previous version of this script)
# are always safe to replace.
if [ -d "$DEST" ] && [ ! -L "$DEST" ] && [ ! -e "$DEST/$SENTINEL" ]; then
    echo "$DEST exists, isn't ours (no sentinel); refusing to clobber." >&2
    exit 1
fi

rm -rf "$DEST"
cp -r "$SRC" "$DEST"
touch "$DEST/$SENTINEL"
echo "Copied $SRC -> $DEST"
