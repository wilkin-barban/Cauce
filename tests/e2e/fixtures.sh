#!/usr/bin/env bash
# End-to-end fixtures for T23/C-060 (`odd/tasks/cauce-0.1.md`).
#
# Wraps the reproducible generator of T3/C-017 (`tests/fixtures/generate.sh`) with
# the preconditions the real conversions need, and then proves with `ffprobe`
# that the five sources really are what the cases assume:
#
#   sample-h264-aac.mp4   mp4 / h264 + aac      (source for the MKV/H.265 and MP3 cases)
#   sample-hevc.mkv       mkv / hevc + aac      (the H.265 target codec, already muxed)
#   sample-vp9-opus.webm  webm / vp9 + opus     (the synthetic input already in VP9)
#   sample-audio.flac     flac / flac           (source for the FLAC case)
#   sample-audio.mp3      mp3 / mp3             (already the MP3 target codec)
#
# Usage, from the repository root:
#   bash tests/e2e/fixtures.sh [output-dir]     # default: target/cauce-fixtures
#
# Exit codes: 64 usage, 69 missing tool, 65 a fixture is not what it claims.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
output_dir="${1:-$repo_root/target/cauce-fixtures}"

if [[ $# -gt 1 ]]; then
    echo "usage: bash tests/e2e/fixtures.sh [output-dir]" >&2
    exit 64
fi

missing=()
for tool in ffmpeg ffprobe; do
    command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
done
if (( ${#missing[@]} > 0 )); then
    echo "fixtures.sh: missing ${missing[*]}; install them with \`sudo apt install ffmpeg\`" >&2
    exit 69
fi

generator="$repo_root/tests/fixtures/generate.sh"
if [[ ! -f "$generator" ]]; then
    echo "fixtures.sh: the T3 generator is missing at $generator" >&2
    exit 66
fi

# `generate.sh` writes byte-identical outputs across runs on the same build.
bash "$generator" "$output_dir"

# Prints the codec name of the first stream of `kind` (`video` or `audio`), or
# nothing when the file has no such stream. Blocks keep the field order harmless.
probe_stream() {
    local file="$1" want="$2"
    ffprobe -v error -show_entries stream=codec_type,codec_name \
        -of default=noprint_wrappers=0 "$file" |
        awk -v want="$want" '
            /^\[STREAM\]$/   { type = ""; name = "" }
            /^codec_type=/   { type = substr($0, 12) }
            /^codec_name=/   { name = substr($0, 12) }
            /^\[\/STREAM\]$/ { if (type == want && name != "") { print name; exit } }
        '
}

probe_format() {
    local file="$1" field="$2"
    ffprobe -v error -show_entries "format=$field" \
        -of "default=noprint_wrappers=1:nokey=1" "$file" | head -n 1
}

failures=0

fail() {
    echo "fixtures.sh: $*" >&2
    failures=$((failures + 1))
}

# `expected` is `container|video|audio`, with `-` meaning "this stream must not exist".
check_fixture() {
    local name="$1" expected="$2"
    local path="$output_dir/$name"
    local want_container="${expected%%|*}"
    local rest="${expected#*|}"
    local want_video="${rest%%|*}"
    local want_audio="${rest#*|}"

    [[ -f "$path" ]] || { fail "$name was not generated"; return; }

    local size container video audio duration_ms
    size="$(stat -c %s "$path")"
    container="$(probe_format "$path" format_name)"
    video="$(probe_stream "$path" video)"
    audio="$(probe_stream "$path" audio)"
    duration_ms="$(probe_format "$path" duration | awk '{ printf "%d", $1 * 1000 }')"

    (( size <= 1048576 )) || fail "$name is $size bytes; the manifest caps fixtures at 1 MiB"
    (( duration_ms > 0 )) || fail "$name reports no duration; the fixture is unusable"
    (( duration_ms <= 3000 )) || fail "$name lasts ${duration_ms}ms; the manifest caps 3000ms"
    [[ ",$container," == *",$want_container,"* ]] ||
        fail "$name container is \`$container\`, expected \`$want_container\`"

    if [[ "$want_video" == "-" ]]; then
        [[ -z "$video" ]] || fail "$name must be audio-only but carries video \`$video\`"
    else
        [[ "$video" == "$want_video" ]] ||
            fail "$name video is \`$video\`, expected \`$want_video\`"
    fi

    if [[ "$want_audio" == "-" ]]; then
        [[ -z "$audio" ]] || fail "$name must have no audio but carries \`$audio\`"
    else
        [[ "$audio" == "$want_audio" ]] ||
            fail "$name audio is \`$audio\`, expected \`$want_audio\`"
    fi
}

# The five sources of the task, with the stream shapes the real cases rely on.
# `format_name` for MP4 is `mov,mp4,m4a,3gp,3g2,mj2`, hence the substring match.
check_fixture sample-h264-aac.mp4   "mp4|h264|aac"
check_fixture sample-hevc.mkv       "matroska|hevc|aac"
check_fixture sample-vp9-opus.webm  "webm|vp9|opus"
check_fixture sample-audio.flac     "flac|-|flac"
check_fixture sample-audio.mp3      "mp3|-|mp3"

if (( failures > 0 )); then
    echo "fixtures.sh: $failures fixture check(s) failed" >&2
    exit 65
fi

echo "fixtures.sh: OK 5 fixtures in $output_dir"
