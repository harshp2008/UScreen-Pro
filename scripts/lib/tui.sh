# shellcheck shell=bash
# UScreen installer: the text-mode interface (whiptail or dialog). Headless mode never loads this.
#
# Each function has a plain fallback so a terminal without whiptail still gets a usable installer:
# the same questions, asked with `read`.

# Echoes the dialog program to use, whiptail first, then dialog; nothing if neither is installed.
tui_tool() {
    if command -v whiptail >/dev/null 2>&1; then echo whiptail
    elif command -v dialog >/dev/null 2>&1; then echo dialog
    fi
}

# Interactive means a person is at a terminal: both stdin and stdout are TTYs.
tui_interactive() { [ -t 0 ] && [ -t 1 ]; }

# tui_components: ask which parts to install. Echoes the chosen tags, space separated.
# Tags: core menu extension spawnat system
tui_components() {
    local tool defaults="core menu extension spawnat system"
    tool="$(tui_tool)"
    if [ -n "$tool" ]; then
        local out
        out="$("$tool" --title "UScreen installer" --checklist \
            "Choose what to install (space toggles, Enter confirms)" 20 78 8 \
            core      "UScreen daemon, desktop app, Android APK" ON \
            menu      "Pen long-press menu runtime (GTK 4)" ON \
            extension "GNOME Shell extension (app locking, pen menu)" ON \
            spawnat   "spawn-at: places the pen menu" ON \
            system    "Virtual display driver (EVDI) and input permissions (needs sudo)" ON \
            3>&1 1>&2 2>&3)" || return 1
        # whiptail quotes each tag: "core" "menu"
        echo "$out" | tr -d '"'
        return 0
    fi
    echo "UScreen installer — components (default: all)" >&2
    local answer
    printf 'Install which? [%s]: ' "$defaults" >&2
    read -r answer || answer=""
    echo "${answer:-$defaults}"
}

# tui_confirm "question": yes/no, default yes.
tui_confirm() {
    local tool
    tool="$(tui_tool)"
    if [ -n "$tool" ]; then
        "$tool" --title "UScreen installer" --yesno "$1" 12 70
        return
    fi
    local a
    printf '%s [Y/n]: ' "$1" >&2
    read -r a || a=y
    case "$a" in n|N|no|NO) return 1 ;; *) return 0 ;; esac
}

# tui_message "text": a final notice (what to restart).
tui_message() {
    local tool
    tool="$(tui_tool)"
    if [ -n "$tool" ]; then
        "$tool" --title "UScreen installer" --msgbox "$1" 16 74
    else
        printf '\n%s\n\n' "$1" >&2
    fi
}
