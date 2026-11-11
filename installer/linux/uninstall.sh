#!/bin/sh
# Fjerner KronoLog, som install.sh installerede det (og Time Snapper, som det
# hed før). Dine data i ~/.local/share/kronolog bliver liggende.
set -eu

data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}"

rm -f "$HOME/.local/bin/kronolog" \
    "$data_dir/applications/kronolog.desktop" \
    "$data_dir/icons/hicolor/256x256/apps/kronolog.png" \
    "$config_dir/autostart/kronolog.desktop" \
    "$HOME/.local/bin/time-snapper" \
    "$data_dir/applications/timesnapper.desktop" \
    "$data_dir/icons/hicolor/256x256/apps/timesnapper.png" \
    "$config_dir/autostart/time-snapper.desktop"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$data_dir/applications" || true

echo "KronoLog er fjernet. Dine data ligger stadig i $data_dir/kronolog"
