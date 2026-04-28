#!/usr/bin/env bash
# Symmetric tear-down for `setup-board.sh`. Removes the
# `boards/px4/renode-h743/` directory the setup script copied into
# PX4-Autopilot, after verifying it's ours via the sentinel file.
# Idempotent: a no-op if the directory is already absent.

set -euo pipefail

PX4="${PX4_AUTOPILOT_DIR:-$HOME/repos/PX4-Autopilot}"
DEST="$PX4/boards/px4/renode-h743"
SENTINEL=".px4-rs-renode-h743"

if [ ! -e "$DEST" ]; then
    echo "Nothing to do; $DEST doesn't exist."
    exit 0
fi

# Symlink (legacy) — always safe to drop.
if [ -L "$DEST" ]; then
    rm -f "$DEST"
    echo "Removed legacy symlink at $DEST"
    exit 0
fi

# Copy with sentinel — drop. Without sentinel, refuse.
if [ ! -e "$DEST/$SENTINEL" ]; then
    echo "$DEST has no sentinel; refusing to delete." >&2
    exit 1
fi

rm -rf "$DEST"
echo "Removed $DEST"
