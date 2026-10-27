#!/bin/sh
# Install (or with --uninstall remove) the weekly oma-cleanup systemd user timer.
# Review `scripts/oma-cleanup.sh` preview output before enabling it: the timer
# runs with --apply. Usage: scripts/install-cleanup-timer.sh [CHECKOUT] [--uninstall]
set -eu
units="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
if [ "${1:-}" = "--uninstall" ] || [ "${2:-}" = "--uninstall" ]; then
  systemctl --user disable --now oma-cleanup.timer 2>/dev/null || true
  rm -f "$units/oma-cleanup.service" "$units/oma-cleanup.timer" "${XDG_DATA_HOME:-$HOME/.local/share}/omadesign/oma-cleanup.sh"
  systemctl --user daemon-reload
  exit 0
fi
here=$(cd "$(dirname "$0")/.." && pwd)
checkout=$(cd "${1:-$here}" && git rev-parse --path-format=absolute --git-common-dir)
checkout=${checkout%/.git}
# The main checkout may be on any branch, so the timer runs an installed copy
# of this script instead of the checkout's scripts/ directory.
bin="${XDG_DATA_HOME:-$HOME/.local/share}/omadesign/oma-cleanup.sh"
mkdir -p "$units" "$(dirname "$bin")"
install -m 755 "$here/scripts/oma-cleanup.sh" "$bin"
sed -e "s|^WorkingDirectory=.*|WorkingDirectory=$checkout|" \
    -e "s|^ExecStart=.*|ExecStart=/usr/bin/env bash $bin --apply|" \
    "$here/scripts/systemd/oma-cleanup.service" > "$units/oma-cleanup.service"
cp "$here/scripts/systemd/oma-cleanup.timer" "$units/oma-cleanup.timer"
systemctl --user daemon-reload
systemctl --user enable --now oma-cleanup.timer
systemctl --user list-timers oma-cleanup.timer --no-pager
