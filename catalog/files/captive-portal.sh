#!/bin/sh
# Managed by Bindle
# NetworkManager dispatcher: when a network wants a sign-in page first (a
# hotel or café's Wi-Fi), tell whoever is logged in to the desktop, with a
# button that opens the page. Desktops like GNOME and Plasma do this
# themselves; this is for the ones that don't.
[ "$2" = connectivity-change ] || exit 0
[ "${CONNECTIVITY_STATE:-}" = PORTAL ] || exit 0
# Any plain-http address works: the portal answers it with its own page.
url=http://ping.archlinux.org
for s in $(loginctl list-sessions --no-legend | awk '{ print $1 }'); do
    [ "$(loginctl show-session "$s" -p Active --value)" = yes ] || continue
    case "$(loginctl show-session "$s" -p Type --value)" in wayland | x11) ;; *) continue ;; esac
    uid=$(loginctl show-session "$s" -p User --value)
    user=$(id -nu "$uid") || continue
    bus="unix:path=/run/user/$uid/bus"
    # In the background: notify-send waits for the click. $1 is the inner shell's.
    # shellcheck disable=SC2016
    setsid runuser -u "$user" -- env DBUS_SESSION_BUS_ADDRESS="$bus" sh -c '
        answer=$(notify-send --app-name=Network --action=open="Sign in" \
            "This network needs you to sign in" "Open its sign-in page to get online.")
        [ "$answer" = open ] && exec xdg-open "$1"' sh "$url" >/dev/null 2>&1 &
done
exit 0
