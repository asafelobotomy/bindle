#!/bin/sh
# Managed by Bindle
# Add Bindle's commands to CopyQ (~/.config/copyq/copyq-commands.ini):
#  - every copy is stamped with when it was copied, and items older than the
#    chosen age are removed (pinned ones stay);
#  - the tray menu offers "Keep clipboard history: 30 days / 60 days /
#    90 days / forever" (30 days to start with).
# A re-run replaces Bindle's commands (their scripts use "bindle-"), keeping
# the rest.
#   copyq-commands            add them
#   copyq-commands --remove   take them out again
# CopyQ reads its commands when it starts; a running CopyQ is restarted.
set -eu
mode=${1:-add}
dir=${XDG_CONFIG_HOME:-$HOME/.config}/copyq
file=$dir/copyq-commands.ini
mkdir -p "$dir"
[ -e "$file" ] || printf '[Commands]\nsize=0\n' >"$file"

# One line per command, fields separated by tabs: name, automatic, in menu,
# icon, script. Scripts are one line, with single quotes only (QSettings).
keep() { # <menu label> <days> <popup text>
    printf 'Keep clipboard history: %s\tfalse\ttrue\t\\xf017\tcopyq: settings('"'"'bindle-keep-days'"'"', %s); popup('"'"'Clipboard history'"'"', '"'"'%s'"'"')\n' "$1" "$2" "$3"
}
stamp="copyq: var key = 'application/x-bindle-copied'; setData(key, String(Date.now())); var days = Number(settings('bindle-keep-days') || 30); if (days > 0) { var cutoff = Date.now() - days * 86400000; tab(config('clipboard_tab')); for (var i = size() - 1; i >= 0; --i) { var t = Number(str(read(key, i))) || 0; if (!t) { change(i, key, String(Date.now())); continue; } if (t < cutoff && !str(read('application/x-copyq-item-pinned', i))) remove(i); } }"
ours() {
    [ "$mode" = --remove ] && return 0
    printf 'Remember when each item was copied\ttrue\tfalse\t\\xf017\t%s\n' "$stamp"
    keep "30 days" 30 "Items are removed 30 days after they were copied."
    keep "60 days" 60 "Items are removed 60 days after they were copied."
    keep "90 days" 90 "Items are removed 90 days after they were copied."
    keep "forever" 0 "Items are kept until you remove them."
}

tmp=$(mktemp "$file.bindle.XXXXXX")
trap 'rm -f "$tmp"' EXIT
ours | awk -v FS='\t' -v file="$file" '
    # Bindle commands to add, from stdin.
    { n++; name[n] = $1; auto[n] = $2; menu[n] = $3; icon[n] = $4; cmd[n] = $5 }
    END {
        # The existing commands, grouped by number; Bindle'"'"'s are dropped.
        while ((getline line < file) > 0) {
            if (line ~ /^\[/) { section = line; if (section != "[Commands]") other = other line "\n"; continue }
            if (section != "[Commands]") { other = other line "\n"; continue }
            if (line ~ /^size=/ || line == "") continue
            split(line, kv, "\\"); i = kv[1] + 0
            if (!(i in seen)) { seen[i] = 1; order[++count] = i }
            body[i] = body[i] substr(line, length(kv[1]) + 2) "\n"
            if (index(line, kv[1] "\\Command=") == 1 && index(line, "bindle-") > 0) drop[i] = 1
        }
        print "[Commands]"
        k = 0
        for (j = 1; j <= count; j++) {
            i = order[j]
            if (i in drop) continue
            k++
            m = split(body[i], lines, "\n")
            for (l = 1; l < m; l++) print k "\\" lines[l]
        }
        for (j = 1; j <= n; j++) {
            k++
            print k "\\Automatic=" auto[j]
            print k "\\Command=\"" cmd[j] "\""
            print k "\\Icon=" icon[j]
            print k "\\InMenu=" menu[j]
            print k "\\Name=\"" name[j] "\""
        }
        print "size=" k
        if (other != "") printf "\n%s", other
    }' >"$tmp"
mv -f "$tmp" "$file"

if command -v copyq >/dev/null 2>&1 && copyq --version >/dev/null 2>&1 && pgrep -u "$(id -u)" -x copyq >/dev/null 2>&1; then
    copyq exit >/dev/null 2>&1 || true
    (setsid copyq >/dev/null 2>&1 &)
fi
