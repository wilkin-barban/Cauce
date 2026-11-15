#!/usr/bin/env bash
# Generates the reproducible media fixtures used by the Cauce test suite.
#
# Fixtures come from synthetic sources (testsrc2/sine) and never from sampled
# media, so nothing copyrighted is stored in the repository. Every output is
# written with bitexact flags and stripped metadata so two runs on the same
# FFmpeg build are byte-identical.
set -euo pipefail

output_dir="${1:-}"
if [[ -z "$output_dir" ]]; then
    echo "usage: $0 <output-dir>" >&2
    exit 64
fi

if ! command -v ffmpeg >/dev/null 2>&1; then
    echo "ffmpeg is required to generate the Cauce fixtures" >&2
    exit 69
fi

mkdir -p "$output_dir"

# Applied to every output: no timestamps, no encoder banners, no source metadata.
deterministic=(-fflags +bitexact -flags +bitexact -map_metadata -1)

emit() {
    local output="$1"
    shift
    ffmpeg -hide_banner -loglevel error -y "$@" "${deterministic[@]}" "$output_dir/$output"
}

# H.264 + AAC in MP4: the baseline compatibility target.
emit sample-h264-aac.mp4 \
    -f lavfi -i "testsrc2=size=320x240:rate=25:duration=2" \
    -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=2" \
    -map 0:v:0 -map 1:a:0 \
    -c:v libx264 -preset veryfast -crf 32 -pix_fmt yuv420p \
    -c:a aac -b:a 64k -shortest -movflags +faststart

# H.265 in Matroska: the efficient-quality target.
emit sample-hevc.mkv \
    -f lavfi -i "testsrc2=size=320x240:rate=25:duration=2" \
    -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=2" \
    -map 0:v:0 -map 1:a:0 \
    -c:v libx265 -preset ultrafast -crf 34 -pix_fmt yuv420p -x265-params log-level=error \
    -c:a aac -b:a 64k -shortest

# VP9 + Opus in WebM: the open-codec target.
emit sample-vp9-opus.webm \
    -f lavfi -i "testsrc2=size=320x240:rate=25:duration=2" \
    -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=2" \
    -map 0:v:0 -map 1:a:0 \
    -c:v libvpx-vp9 -crf 40 -b:v 0 -deadline realtime -cpu-used 8 \
    -c:a libopus -b:a 48k -shortest

# Audio-only sources for the extract-audio presets.
emit sample-audio.flac \
    -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=2" \
    -c:a flac -compression_level 5

emit sample-audio.mp3 \
    -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=2" \
    -c:a libmp3lame -b:a 96k

echo "generated fixtures in $output_dir"
