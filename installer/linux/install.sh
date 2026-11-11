#!/bin/sh
# Installerer KronoLog for den aktuelle bruger (kræver ikke root):
#   ~/.local/bin/kronolog                      programmet
#   ~/.local/share/applications/…desktop       genvej i programmenuen
#   ~/.local/share/icons/hicolor/…             ikon
# Kør igen for at opdatere. Dine data i ~/.local/share/kronolog røres ikke.
# Programmet hed tidligere Time Snapper; dets filer fjernes her, og KronoLog
# flytter selv dataene fra ~/.local/share/timesnapper ved første start.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin_dir="$HOME/.local/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}"
exe="$bin_dir/kronolog"

# Stier i Exec= skal citeres og have \ " ` $ escapet (Desktop Entry-specifikationen).
exec_line=$(printf '%s' "$exe" | sed 's/[\\"`$]/\\&/g')

install -Dm755 "$here/kronolog" "$exe"
install -Dm644 "$here/kronolog.png" "$data_dir/icons/hicolor/256x256/apps/kronolog.png"
mkdir -p "$data_dir/applications"
cat > "$data_dir/applications/kronolog.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=KronoLog
Comment=Tidsregistrering for kunder og projekter
Exec="$exec_line"
Icon=kronolog
Terminal=false
Categories=Office;
StartupWMClass=kronolog
DESKTOP

# Time Snapper (det gamle navn): fjern programmet, genvejen og ikonet.
rm -f "$bin_dir/time-snapper" \
    "$data_dir/applications/timesnapper.desktop" \
    "$data_dir/icons/hicolor/256x256/apps/timesnapper.png"

# Er automatisk start slået til (fra en tidligere version, også under det
# gamle navn), så peg den på den installerede version.
autostart="$config_dir/autostart/kronolog.desktop"
old_autostart="$config_dir/autostart/time-snapper.desktop"
if [ -f "$old_autostart" ]; then
    mv "$old_autostart" "$autostart"
    sed -i 's/^Name=.*/Name=KronoLog/' "$autostart"
fi
if [ -f "$autostart" ]; then
    sed -i "s|^Exec=.*|Exec=\"$(printf '%s' "$exec_line" | sed 's/[|&\\]/\\&/g')\"|" "$autostart"
fi

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$data_dir/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$data_dir/icons/hicolor" 2>/dev/null || true

echo "KronoLog er installeret i $exe"
echo "Start det fra programmenuen, eller kør: kronolog"
case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) echo "Bemærk: $bin_dir er ikke i din PATH, så kommandoen virker først efter næste login." ;;
esac
