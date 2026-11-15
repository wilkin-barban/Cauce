#!/usr/bin/env bash
# T24 / C-061: GUI + daemon smoke under Xvfb (`odd/tasks/cauce-0.1.md`).
#
# What this proves, in order, with observable conditions only (never a sleep as
# an assertion — every wait is a bounded poll):
#
#   1. The daemon binds `$XDG_RUNTIME_DIR/cauce/daemon.sock` with mode 0600.
#   2. `cauce-gui` opens a real window under Xvfb: `CAUCE_SMOKE=1` prints
#      `window-ready` on the first map and then quits by itself.
#   3. Closing the GUI does NOT stop the daemon: the process and the socket are
#      still there after the GUI exits (`docs/architecture.md` §19, S9).
#   4. A real conversion queued through `tests/e2e/real_conversions.py` completes
#      while only the daemon is running, and the published file validates.
#   5. `SIGTERM` stops the daemon cleanly (exit 0) and the socket is removed.
#
# Usage, from the repository root:
#   bash tests/e2e/gui_smoke.sh
#
# Binaries: `CAUCE_DAEMON_BIN` / `CAUCE_GUI_BIN` override the lookup, otherwise
# target/debug, target/release and /usr/bin are tried in that order. The script
# never builds anything; it fails with the exact `cargo build` command instead.
#
# Exit codes: 69 missing tool or binary, 70 the daemon never answered,
# 71 the GUI smoke failed, 72 the conversion failed, 73 the daemon did not stop.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
only_preset="${CAUCE_SMOKE_PRESET:-system-smarttv-compat}"
daemon_budget_s=30
gui_budget_s=120
stop_budget_s=20

fail() {
    local code="$1"
    shift
    echo "gui_smoke: $*" >&2
    exit "$code"
}

# --- preconditions ---------------------------------------------------------
missing=()
for tool in xvfb-run Xvfb ffmpeg ffprobe python3; do
    command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
done
if (( ${#missing[@]} > 0 )); then
    fail 69 "missing ${missing[*]}; install them with \`sudo apt install xvfb ffmpeg python3\`"
fi

# The daemon only authorizes paths inside $HOME (`docs/contracts.md` §9), so the
# work folder has to live there too.
[[ -n "${HOME:-}" && -d "$HOME" ]] || fail 69 "HOME must point at an existing directory"

resolve_binary() {
    local override="$1"
    shift
    if [[ -n "$override" ]]; then
        printf '%s\n' "$override"
        return 0
    fi
    local candidate
    for candidate in "$@"; do
        if [[ -x "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

for override in "${CAUCE_DAEMON_BIN:-}" "${CAUCE_GUI_BIN:-}"; do
    if [[ -n "$override" && ! -x "$override" ]]; then
        fail 69 "$override is not an executable"
    fi
done

daemon_bin="$(resolve_binary "${CAUCE_DAEMON_BIN:-}" \
    "$repo_root/target/debug/cauce-daemon" \
    "$repo_root/target/release/cauce-daemon" \
    /usr/bin/cauce-daemon)" ||
    fail 69 "cauce-daemon not found; build it with \`cargo build -p cauce-daemon\` or install the .deb"
gui_bin="$(resolve_binary "${CAUCE_GUI_BIN:-}" \
    "$repo_root/target/debug/cauce-gui" \
    "$repo_root/target/release/cauce-gui" \
    /usr/bin/cauce-gui)" ||
    fail 69 "cauce-gui not found; build it with \`cargo build -p cauce-gui\` or install the .deb"

# --- isolated runtime ------------------------------------------------------
work="$(mktemp -d "$HOME/.cauce-e2e-XXXXXX")"
mkdir -p "$work/run" "$work/data" "$work/fixtures" "$work/out"
chmod 700 "$work/run"
export XDG_RUNTIME_DIR="$work/run"
export XDG_DATA_HOME="$work/data"
socket="$XDG_RUNTIME_DIR/cauce/daemon.sock"
export CAUCE_FIXTURES="$work/fixtures"

daemon_pid=""
cleanup() {
    if [[ -n "$daemon_pid" ]] && kill -0 "$daemon_pid" 2>/dev/null; then
        kill -TERM "$daemon_pid" 2>/dev/null || true
        wait "$daemon_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT

echo "gui_smoke: fixtures in $work/fixtures"
bash "$repo_root/tests/e2e/fixtures.sh" "$CAUCE_FIXTURES" ||
    fail 69 "tests/e2e/fixtures.sh failed; the smoke cannot run without fixtures"

# The smoke owns the daemon, so an unknown daemon already answering on our socket
# path is a hard error, never something to reuse or to overwrite.
if [[ -e "$socket" ]]; then
    fail 70 "$socket already exists; refusing to run against an unknown daemon"
fi

# --- 1. the daemon binds a private socket ----------------------------------
echo "gui_smoke: starting $daemon_bin"
"$daemon_bin" >"$work/daemon.log" 2>&1 &
daemon_pid=$!

deadline=$((SECONDS + daemon_budget_s))
while [[ ! -S "$socket" ]]; do
    if ! kill -0 "$daemon_pid" 2>/dev/null; then
        cat "$work/daemon.log" >&2
        fail 70 "cauce-daemon exited before binding $socket"
    fi
    (( SECONDS < deadline )) || {
        cat "$work/daemon.log" >&2
        fail 70 "no socket at $socket after ${daemon_budget_s}s"
    }
    sleep 0.1
done
socket_mode="$(stat -c %a "$socket")"
[[ "$socket_mode" == "600" ]] ||
    fail 70 "the socket mode is $socket_mode, expected 600 (docs/contracts.md §5)"

# --- 2. the GUI opens a window under Xvfb ----------------------------------
echo "gui_smoke: running $gui_bin under Xvfb (CAUCE_SMOKE=1)"
gui_status=0
CAUCE_SMOKE=1 xvfb-run -a "$gui_bin" >"$work/gui.log" 2>&1 || gui_status=$?
if ! grep -q 'window-ready' "$work/gui.log"; then
    cat "$work/gui.log" >&2
    fail 71 "cauce-gui did not print \`window-ready\` under Xvfb (exit $gui_status)"
fi
(( gui_status == 0 )) || {
    cat "$work/gui.log" >&2
    fail 71 "cauce-gui exited $gui_status after printing \`window-ready\`"
}

# --- 3. the daemon survives the GUI ---------------------------------------
kill -0 "$daemon_pid" 2>/dev/null ||
    fail 70 "the daemon died when the GUI closed; conversions must survive the GUI (S9)"
[[ -S "$socket" ]] || fail 70 "the socket disappeared when the GUI closed"

# --- 4. a real conversion completes with only the daemon running ----------
echo "gui_smoke: queueing $only_preset through real_conversions.py"
python3 "$repo_root/tests/e2e/real_conversions.py" "$socket" "$work/out" \
    --only "$only_preset" || fail 72 "the real conversion did not complete"
kill -0 "$daemon_pid" 2>/dev/null || fail 70 "the daemon died during the conversion"

# --- 5. a clean stop -------------------------------------------------------
echo "gui_smoke: stopping the daemon"
kill -TERM "$daemon_pid"
deadline=$((SECONDS + stop_budget_s))
while kill -0 "$daemon_pid" 2>/dev/null; do
    (( SECONDS < deadline )) || {
        cat "$work/daemon.log" >&2
        fail 73 "cauce-daemon ignored SIGTERM for ${stop_budget_s}s"
    }
    sleep 0.1
done
daemon_status=0
wait "$daemon_pid" || daemon_status=$?
daemon_pid=""
(( daemon_status == 0 )) || fail 73 "cauce-daemon exited $daemon_status after SIGTERM"
[[ ! -S "$socket" ]] || fail 73 "the socket $socket outlived the daemon"

echo "gui_smoke: OK  window-ready, daemon survived the GUI, $only_preset converted, clean stop"
echo "gui_smoke: logs kept in $work (remove it when the evidence is archived)"
