#!/usr/bin/env bash
#
# Run BananaTray's GNOME Shell Extension in a nested Wayland GNOME Shell.
#
# This avoids logging out of the real desktop while iterating on extension.js,
# stylesheet.css, and D-Bus UI behavior.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

UUID="bananatray@bananatray.github.io"
EXTENSION_SRC="$PROJECT_DIR/gnome-shell-extension"
MOCK_DAEMON="$SCRIPT_DIR/gnome-extension-mock-daemon.js"

PROFILE_DIR=""
DAEMON_MODE="mock"
SHELL_ARGS=("--devkit" "--wayland" "--no-x11")
KEEP_PROFILE=false
APP_COMMAND_STRING=""
REAL_XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-}"
REAL_XDG_DATA_HOME="${XDG_DATA_HOME:-}"
REAL_XDG_CACHE_HOME="${XDG_CACHE_HOME:-}"
REAL_XDG_STATE_HOME="${XDG_STATE_HOME:-}"

usage() {
    cat <<'EOF'
Usage: bash scripts/dev-gnome-extension.sh [OPTIONS] [-- GNOME_SHELL_ARGS...]

Options:
  --real-daemon        Do not start a daemon. Use this when you start BananaTray
                       yourself inside the nested D-Bus session.
  --app-daemon         Start the real BananaTray app with cargo run inside the
                       nested D-Bus session, using your normal app settings.
  --app-command CMD    Command used by --app-daemon. Defaults to: cargo run.
  --profile-dir DIR    Reuse a specific temporary profile/config directory.
  --monitor WxH[@R]    Add an explicit nested virtual monitor.
  -h, --help           Show this help.

Examples:
  bash scripts/dev-gnome-extension.sh
  bash scripts/dev-gnome-extension.sh --monitor 1600x1000
  bash scripts/dev-gnome-extension.sh --app-daemon
  bash scripts/dev-gnome-extension.sh --app-command 'cargo run --release'
  bash scripts/dev-gnome-extension.sh --real-daemon -- --force-animations

Notes:
  - This runs under dbus-run-session, so it has an isolated session bus.
  - The default mock daemon implements com.bananatray.Daemon for extension UI work.
  - --app-daemon runs BananaTray on the same nested bus but preserves your real
    XDG_CONFIG_HOME so provider settings and credentials are read normally.
  - GNOME Shell 49+ needs mutter-devkit for a visible nested Shell window.
  - metadata.json changes are picked up by restarting this nested shell, not by
    logging out of the real desktop.
EOF
}

require_command() {
    local name="$1"
    if ! command -v "$name" >/dev/null 2>&1; then
        echo "Missing required command: $name" >&2
        exit 1
    fi
}

require_mutter_devkit() {
    if [[ -x /usr/libexec/mutter-devkit ]] || command -v mutter-devkit >/dev/null 2>&1; then
        return
    fi

    cat >&2 <<'EOF'
Missing mutter-devkit.

gnome-shell --devkit --wayland can start without it, but the nested Shell has
no visible host window. On Ubuntu/Debian install:

  sudo apt install mutter-dev-bin

Then rerun:

  bash scripts/dev-gnome-extension.sh
EOF
    exit 1
}

copy_extension() {
    local extension_dir="$1"

    rm -rf "$extension_dir"
    mkdir -p "$(dirname "$extension_dir")"
    cp -a "$EXTENSION_SRC" "$extension_dir"
}

prepare_profile_environment() {
    mkdir -p \
        "$PROFILE_DIR/config" \
        "$PROFILE_DIR/data" \
        "$PROFILE_DIR/cache" \
        "$PROFILE_DIR/state" \
        "$PROFILE_DIR/dconf-profile"

    # dconf is D-Bus activated. The profile file and environment must exist
    # before dbus-run-session starts, otherwise the activated dconf service will
    # write to the real user profile instead of this temporary one.
    printf 'user-db:user\n' > "$PROFILE_DIR/dconf-profile/user"
}

cleanup() {
    if [[ -n "${MOCK_PID:-}" ]]; then
        kill "$MOCK_PID" >/dev/null 2>&1 || true
        wait "$MOCK_PID" >/dev/null 2>&1 || true
    fi
    if [[ -n "${APP_PID:-}" ]]; then
        kill "$APP_PID" >/dev/null 2>&1 || true
        wait "$APP_PID" >/dev/null 2>&1 || true
    fi

    if [[ -n "$PROFILE_DIR" && "${KEEP_PROFILE:-false}" != "true" ]]; then
        rm -rf "$PROFILE_DIR"
    fi
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --real-daemon)
                DAEMON_MODE="external"
                shift
                ;;
            --app-daemon)
                DAEMON_MODE="app"
                shift
                ;;
            --app-command)
                local command="${2:-}"
                if [[ -z "$command" ]]; then
                    echo "--app-command requires a command string" >&2
                    exit 1
                fi
                DAEMON_MODE="app"
                APP_COMMAND_STRING="$command"
                shift 2
                ;;
            --profile-dir)
                PROFILE_DIR="${2:-}"
                if [[ -z "$PROFILE_DIR" ]]; then
                    echo "--profile-dir requires a directory" >&2
                    exit 1
                fi
                KEEP_PROFILE=true
                shift 2
                ;;
            --monitor)
                local monitor="${2:-}"
                if [[ -z "$monitor" ]]; then
                    echo "--monitor requires a size such as 1280x800" >&2
                    exit 1
                fi
                SHELL_ARGS+=("--virtual-monitor" "$monitor")
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            --)
                shift
                SHELL_ARGS+=("$@")
                break
                ;;
            *)
                echo "Unknown option: $1" >&2
                usage >&2
                exit 1
                ;;
        esac
    done
}

wait_for_daemon() {
    local label="$1"
    local attempts="${2:-50}"
    local pid="${3:-}"

    for _ in $(seq 1 "$attempts"); do
        if gdbus introspect \
            --session \
            --dest com.bananatray.Daemon \
            --object-path /com/bananatray/Daemon >/dev/null 2>&1; then
            return 0
        fi
        if [[ -n "$pid" ]] && ! kill -0 "$pid" >/dev/null 2>&1; then
            echo "$label process exited before registering com.bananatray.Daemon" >&2
            return 1
        fi
        sleep 0.1
    done

    echo "$label did not appear on the nested session bus" >&2
    return 1
}

start_mock_daemon() {
    GIO_USE_VFS=local gjs "$MOCK_DAEMON" &
    MOCK_PID="$!"
    wait_for_daemon "Mock daemon" 50 "$MOCK_PID"
}

start_app_daemon() {
    local saved_xdg_config_home="$BANANATRAY_REAL_XDG_CONFIG_HOME"
    local saved_xdg_data_home="$BANANATRAY_REAL_XDG_DATA_HOME"
    local saved_xdg_cache_home="$BANANATRAY_REAL_XDG_CACHE_HOME"
    local saved_xdg_state_home="$BANANATRAY_REAL_XDG_STATE_HOME"

    local app_command=()
    if [[ -n "$APP_COMMAND_STRING" ]]; then
        app_command=("bash" "-lc" "$APP_COMMAND_STRING")
    else
        APP_COMMAND_STRING="cargo run"
        app_command=("cargo" "run")
    fi

    (
        cd "$PROJECT_DIR"
        export BANANATRAY_FORCE_GNOME_EXTENSION="1"
        export BANANATRAY_SINGLE_INSTANCE_SUFFIX="gnome-dev"

        if [[ -n "$saved_xdg_config_home" ]]; then
            export XDG_CONFIG_HOME="$saved_xdg_config_home"
        else
            unset XDG_CONFIG_HOME
        fi
        if [[ -n "$saved_xdg_data_home" ]]; then
            export XDG_DATA_HOME="$saved_xdg_data_home"
        else
            unset XDG_DATA_HOME
        fi
        if [[ -n "$saved_xdg_cache_home" ]]; then
            export XDG_CACHE_HOME="$saved_xdg_cache_home"
        else
            unset XDG_CACHE_HOME
        fi
        if [[ -n "$saved_xdg_state_home" ]]; then
            export XDG_STATE_HOME="$saved_xdg_state_home"
        else
            unset XDG_STATE_HOME
        fi

        exec "${app_command[@]}"
    ) &
    APP_PID="$!"
    wait_for_daemon "BananaTray app daemon" 1800 "$APP_PID"
}

run_nested_session() {
    export XDG_CONFIG_HOME="$PROFILE_DIR/config"
    export XDG_DATA_HOME="$PROFILE_DIR/data"
    export XDG_CACHE_HOME="$PROFILE_DIR/cache"
    export XDG_STATE_HOME="$PROFILE_DIR/state"
    export DCONF_PROFILE="$PROFILE_DIR/dconf-profile/user"

    local extension_dir="$XDG_DATA_HOME/gnome-shell/extensions/$UUID"
    copy_extension "$extension_dir"

    gsettings set org.gnome.shell disable-user-extensions false
    gsettings set org.gnome.shell enabled-extensions "['$UUID']"

    if [[ "$(gsettings get org.gnome.shell enabled-extensions)" != *"$UUID"* ]]; then
        echo "Failed to enable $UUID in nested GNOME profile" >&2
        exit 1
    fi

    case "$DAEMON_MODE" in
        mock)
            start_mock_daemon
            ;;
        app)
            start_app_daemon
            ;;
        external)
            ;;
        *)
            echo "Unknown daemon mode: $DAEMON_MODE" >&2
            exit 1
            ;;
    esac

    echo "Nested GNOME Shell profile: $PROFILE_DIR"
    echo "Extension installed at: $extension_dir"
    echo "Daemon mode: $DAEMON_MODE"
    if [[ "$DAEMON_MODE" == "app" ]]; then
        echo "App command: $APP_COMMAND_STRING"
    fi
    echo "Starting: gnome-shell ${SHELL_ARGS[*]}"
    echo

    if [[ "${BANANATRAY_GNOME_DRY_RUN:-false}" == "true" ]]; then
        echo "Dry run enabled; not starting nested GNOME Shell."
        return 0
    fi

    gnome-shell "${SHELL_ARGS[@]}"
}

main() {
    parse_args "$@"

    require_command dbus-run-session
    require_command gnome-shell
    require_command gsettings
    require_command gdbus
    require_mutter_devkit

    if [[ "$DAEMON_MODE" == "mock" ]]; then
        require_command gjs
    elif [[ "$DAEMON_MODE" == "app" && -z "$APP_COMMAND_STRING" ]]; then
        require_command cargo
    fi

    if [[ -z "$PROFILE_DIR" ]]; then
        PROFILE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/bananatray-gnome-dev.XXXXXX")"
        KEEP_PROFILE=false
    else
        mkdir -p "$PROFILE_DIR"
    fi

    prepare_profile_environment

    trap cleanup EXIT INT TERM

    XDG_CONFIG_HOME="$PROFILE_DIR/config" \
    XDG_DATA_HOME="$PROFILE_DIR/data" \
    XDG_CACHE_HOME="$PROFILE_DIR/cache" \
    XDG_STATE_HOME="$PROFILE_DIR/state" \
    DCONF_PROFILE="$PROFILE_DIR/dconf-profile/user" \
    BANANATRAY_REAL_XDG_CONFIG_HOME="$REAL_XDG_CONFIG_HOME" \
    BANANATRAY_REAL_XDG_DATA_HOME="$REAL_XDG_DATA_HOME" \
    BANANATRAY_REAL_XDG_CACHE_HOME="$REAL_XDG_CACHE_HOME" \
    BANANATRAY_REAL_XDG_STATE_HOME="$REAL_XDG_STATE_HOME" \
    BANANATRAY_GNOME_DEV_CHILD=1 \
    BANANATRAY_GNOME_PROFILE_DIR="$PROFILE_DIR" \
    BANANATRAY_GNOME_DAEMON_MODE="$DAEMON_MODE" \
    BANANATRAY_GNOME_APP_COMMAND="$APP_COMMAND_STRING" \
    BANANATRAY_GNOME_KEEP_PROFILE="$KEEP_PROFILE" \
        dbus-run-session -- bash "$0" "${SHELL_ARGS[@]}"
}

if [[ "${BANANATRAY_GNOME_DEV_CHILD:-}" == "1" ]]; then
    PROFILE_DIR="${BANANATRAY_GNOME_PROFILE_DIR:?missing nested profile dir}"
    DAEMON_MODE="${BANANATRAY_GNOME_DAEMON_MODE:-mock}"
    APP_COMMAND_STRING="${BANANATRAY_GNOME_APP_COMMAND:-}"
    KEEP_PROFILE="${BANANATRAY_GNOME_KEEP_PROFILE:-false}"
    SHELL_ARGS=("$@")
    trap cleanup EXIT INT TERM
    run_nested_session
else
    main "$@"
fi
