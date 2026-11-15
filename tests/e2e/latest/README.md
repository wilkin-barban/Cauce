# Cauce 0.1 — end-to-end evidence

Scripts, not mocks: every check below talks to a real `cauce-daemon`, real
`ffmpeg`/`ffprobe` and (for the GUI) a real window under Xvfb. They are the
evidence behind the acceptance criteria of `docs/architecture.md` §19 and tasks
T23-T25 of `odd/tasks/cauce-0.1.md`.

All commands run **from the repository root**.

## Prerequisites

| Need | Check | Install |
|---|---|---|
| `ffmpeg` + `ffprobe` | `ffprobe -version` | `sudo apt install ffmpeg` |
| `python3` (stdlib only) | `python3 --version` | `sudo apt install python3` |
| `xvfb-run` + `Xvfb` | `command -v xvfb-run Xvfb` | `sudo apt install xvfb` |
| `cauce-daemon` + `cauce-gui` | `ls target/debug/cauce-*` | `cargo build --workspace` or install the `.deb` |

`gui_smoke.sh` finds the binaries in `target/debug`, `target/release` and
`/usr/bin`, in that order; override with `CAUCE_DAEMON_BIN` / `CAUCE_GUI_BIN`.
It never builds and never publishes anything.

## 1. Fixtures — `tests/e2e/fixtures.sh`

```bash
bash tests/e2e/fixtures.sh                 # -> target/cauce-fixtures
bash tests/e2e/fixtures.sh /tmp/fixtures   # anywhere else
```

Wraps the reproducible generator of T3 (`tests/fixtures/generate.sh`) and then
proves with `ffprobe` that the five sources are what the cases assume:

| Fixture | Container / streams |
|---|---|
| `sample-h264-aac.mp4` | MP4, `h264` + `aac` |
| `sample-hevc.mkv` | Matroska, `hevc` + `aac` |
| `sample-vp9-opus.webm` | WebM, `vp9` + `opus` |
| `sample-audio.flac` | FLAC, audio only |
| `sample-audio.mp3` | MP3, audio only |

It fails (exit 65) if a fixture is larger than 1 MiB, longer than 3 s, or has a
different container/codec than the table above.

## 2. Real conversions — `tests/e2e/real_conversions.py`

```bash
python3 tests/e2e/real_conversions.py <socket> <output-dir>
python3 tests/e2e/real_conversions.py <socket> <output-dir> --only system-audio-mp3
```

It speaks NDJSON JSON-RPC 2.0 to the daemon: `initialize`, one `jobs.add` per
input/preset pair, then `jobs.get` polled until the job is terminal. Every wait
is a poll with a monotonic deadline (`CAUCE_E2E_TIMEOUT`, default 180 s), never a
fixed sleep. Each published file is verified with

```bash
ffprobe -v error -print_format json -show_streams -show_format <output>
```

against: container, video codec (or *no* video for the audio presets), audio
codec, resolution within the preset limit, duration within `max(2 s, 3 %)` of the
source, and the preset channel ceiling. It also asserts the output path is not
the input path and that the input file is byte- and mtime-identical afterwards.

The five cases, with one line per success
(`OK <preset> <file> <codec>/<audio> <duration>ms`):

| # | Preset | Source | Published | Proves |
|---|---|---|---|---|
| 1 | `system-smarttv-compat` | `sample-vp9-opus.webm` | MP4, `h264`/`aac` | full **transcode** VP9/Opus → H.264/AAC |
| 2 | `system-smarttv-hevc` | `sample-h264-aac.mp4` | MKV, `hevc`/`aac` | video **transcode** H.264 → H.265, container change |
| 3 | `system-archival-master` | `sample-vp9-opus.webm` | MKV, `hevc`/`opus` | the **copy** case: Opus audio copied, VP9 video transcoded |
| 4 | `system-audio-mp3` | `sample-h264-aac.mp4` | MP3, `/mp3` | audio preset, video removed, AAC → MP3 |
| 5 | `system-audio-flac` | `sample-audio.flac` | FLAC, `/flac` | audio-only preset, no video, FLAC output |

Case 3 exists because v0.1 has no preset whose video codec is VP9: the WebM/VP9
acceptance criterion is proved from the input side, and the plan copies the Opus
audio instead of re-encoding it (that is why the published audio codec is still
`opus`). Fixtures are generated on demand with `fixtures.sh`; point
`CAUCE_FIXTURES` at an existing folder to reuse them.

Exit codes: 64 usage, 65 a verification failure (the ffprobe JSON evidence is
printed), 66 a missing fixture, 69 a missing tool or socket, 75 the daemon never
answered in time.

## 3. GUI + daemon smoke — `tests/e2e/gui_smoke.sh`

```bash
bash tests/e2e/gui_smoke.sh
```

One run, five observable steps, each with a bounded deadline:

1. The daemon binds `$XDG_RUNTIME_DIR/cauce/daemon.sock` with mode `0600` in an
   isolated runtime dir (`$HOME/.cauce-e2e-XXXXXX`, because the daemon only
   authorizes paths inside `$HOME`).
2. `cauce-gui` opens a window under `xvfb-run -a` with `CAUCE_SMOKE=1` and prints
   `window-ready`; the script fails if that line never appears.
3. After the GUI exits, the daemon process and the socket are **still alive** —
   closing the GUI must not stop the queue (S9).
4. One conversion (`CAUCE_SMOKE_PRESET`, default `system-smarttv-compat`) is
   queued through `real_conversions.py` and must complete with the daemon as the
   only client.
5. `SIGTERM` stops the daemon with exit 0 and the socket is removed.

It fails loudly when a prerequisite is missing or a step does not prove itself:
exit 69 for `xvfb-run`, `Xvfb`, `ffmpeg`, `ffprobe`, `python3` or either binary;
exit 70 when the socket never appears, is not mode `0600`, disappears, or when a
stale socket already exists at the path (the smoke owns the daemon it starts and
refuses to talk to an unknown one); exit 71 for the GUI; 72 for the conversion;
73 when `SIGTERM` does not stop the daemon. Logs are kept in the work folder
printed at the end of the run; nothing is deleted and nothing is published.

## What is still not covered here

- **No HDR fixture**: tone-mapping (`zscale`/`tonemap`) is unit-tested but there
  is no HDR sample in the e2e set, so the HDR→SDR path is not exercised end to
  end.
- **`normalize_audio` stays rejected** with `PLAN_INVALID` (no normalization
  filter in v0.1), so no preset that needs it can run.
- **Queue pause is in memory**: restarting the daemon resumes the queue; only
  the jobs' states are persisted.
- **Only `$HOME` is authorized**: inputs and the output folder must live inside
  it, otherwise the daemon answers `PATH_DENIED`.
