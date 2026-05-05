#!/usr/bin/env bash
#
# Install BananaTray's GNOME Shell Extension into the current user's profile.
#
# The script intentionally installs to the user extension directory only. System
# installation needs package-manager ownership and should stay in bundle scripts.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

UUID="bananatray@bananatray.github.io"
EXTENSION_SRC="$PROJECT_DIR/gnome-shell-extension"
XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
EXTENSION_DIR="$XDG_DATA_HOME/gnome-shell/extensions/$UUID"
DRY_RUN=false
ENABLE_EXTENSION=true
STATUS_ONLY=false

required_files=(
    "metadata.json"
    "extension.js"
    "panelButton.js"
    "quotaClient.js"
    "quotaPresentation.js"
    "quotaWidgets.js"
    "stylesheet.css"
    "icons/bananatray-symbolic.svg"
)

usage() {
    cat <<'EOF'
Usage: bash scripts/install-gnome-extension.sh [OPTIONS]

Options:
  --dry-run       Print the target path and checks without copying files.
  --no-enable     Install files but do not run `gnome-extensions enable`.
  --status        Only print current install and GNOME Shell status.
  -h, --help      Show this help.

After installing updated files, GNOME Shell may still cache a previous extension
load error. On Wayland, log out and log back in; on X11, use Alt+F2, r, Enter.
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --dry-run)
                DRY_RUN=true
                shift
                ;;
            --no-enable)
                ENABLE_EXTENSION=false
                shift
                ;;
            --status)
                STATUS_ONLY=true
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                echo "Unknown option: $1" >&2
                usage >&2
                exit 1
                ;;
        esac
    done
}

ensure_source_files() {
    for file in "${required_files[@]}"; do
        if [[ ! -f "$EXTENSION_SRC/$file" ]]; then
            echo "error: missing source extension file: gnome-shell-extension/$file" >&2
            exit 1
        fi
    done
}

print_install_paths() {
    echo "Extension source: $EXTENSION_SRC"
    echo "Extension target: $EXTENSION_DIR"
}

install_extension_files() {
    if [[ "$DRY_RUN" == "true" ]]; then
        echo "Dry run: not copying extension files."
        return
    fi

    mkdir -p "$EXTENSION_DIR"
    cp -a "$EXTENSION_SRC/." "$EXTENSION_DIR/"
}

check_installed_files() {
    local missing=false

    if [[ ! -d "$EXTENSION_DIR" ]]; then
        echo "Installed files: missing target directory"
        return 1
    fi

    for file in "${required_files[@]}"; do
        if [[ ! -f "$EXTENSION_DIR/$file" ]]; then
            echo "Installed files: missing $file"
            missing=true
        fi
    done

    if [[ "$missing" == "true" ]]; then
        return 1
    fi

    echo "Installed files: ok"
}

enable_extension() {
    if [[ "$ENABLE_EXTENSION" != "true" || "$DRY_RUN" == "true" ]]; then
        return
    fi

    if ! command -v gnome-extensions >/dev/null 2>&1; then
        echo "gnome-extensions not found; skipping enable step"
        return
    fi

    gnome-extensions enable "$UUID"
}

print_gnome_shell_status() {
    if ! command -v gnome-extensions >/dev/null 2>&1; then
        echo "GNOME Shell status: gnome-extensions not found"
        return
    fi

    local info
    if ! info="$(gnome-extensions info "$UUID" 2>&1)"; then
        if [[ -z "$info" ]]; then
            echo "GNOME Shell status: unable to query extension state"
            echo "gnome-extensions produced no output. Run this from the target GNOME user session."
            return
        fi
        echo "GNOME Shell status: $info"
        echo "If files were just installed, reload GNOME Shell so it scans the new extension."
        return
    fi

    echo "$info"

    if grep -q "State: ACTIVE" <<<"$info"; then
        echo "GNOME Shell status: active"
        return
    fi

    if grep -q "State: ERROR" <<<"$info"; then
        print_extension_error_hint
        return
    fi

    echo "GNOME Shell status: not active yet"
    print_reload_hint
}

print_extension_error_hint() {
    if command -v gdbus >/dev/null 2>&1; then
        local error_info
        error_info="$(gdbus call \
            --session \
            --dest org.gnome.Shell \
            --object-path /org/gnome/Shell \
            --method org.gnome.Shell.Extensions.GetExtensionInfo \
            "$UUID" 2>/dev/null || true)"

        if [[ -n "$error_info" ]]; then
            echo "GNOME Shell error detail:"
            echo "$error_info"

            if grep -q "add_actor is not a function" <<<"$error_info" &&
                [[ -f "$EXTENSION_DIR/extension.js" ]] &&
                ! grep -q "add_actor" "$EXTENSION_DIR/extension.js"; then
                echo
                echo "The installed files are updated, but GNOME Shell still reports the old add_actor error."
                print_reload_hint
                return
            fi
        fi
    fi

    print_reload_hint
}

print_reload_hint() {
    local session_type="${XDG_SESSION_TYPE:-unknown}"

    if [[ "$session_type" == "wayland" ]]; then
        echo "Reload required: log out and log back in on Wayland."
    elif [[ "$session_type" == "x11" ]]; then
        echo "Reload required: press Alt+F2, type r, then press Enter on X11."
    else
        echo "Reload required: restart GNOME Shell or log out and log back in."
    fi
}

main() {
    parse_args "$@"
    ensure_source_files
    print_install_paths

    if [[ "$STATUS_ONLY" != "true" ]]; then
        install_extension_files
        enable_extension
    fi

    check_installed_files || true
    print_gnome_shell_status
}

main "$@"
