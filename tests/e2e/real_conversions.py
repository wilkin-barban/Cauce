#!/usr/bin/env python3
"""T23 / C-060: five real conversions through a running Cauce daemon.

Speaks NDJSON JSON-RPC 2.0 to the daemon's Unix socket: `initialize`, then one
`jobs.add` per input/preset pair, then `jobs.get` polled until the job is
terminal. No sleep is ever evidence: every wait is a poll with a monotonic
deadline, and every assertion is a measured `ffprobe` fact about the published
file.

Usage, from the repository root:
    python3 tests/e2e/real_conversions.py <socket> <output-dir>
    [--only <preset-id>]        # run a single case (the GUI smoke uses this)

Fixtures come from `tests/e2e/fixtures.sh` (default `target/cauce-fixtures`,
override with `CAUCE_FIXTURES`) and are generated on demand when missing.

Which case exercises `copy` and which exercises `transcode`
----------------------------------------------------------
v0.1 has no preset whose video codec is VP9, so the "WebM/VP9" acceptance
criterion is proved from the input side: the synthetic `sample-vp9-opus.webm`
goes to `system-archival-master`, whose plan transcodes the VP9 video to H.265
and *copies* the original Opus audio into the Matroska container. That is the
explicit **copy** case (`AudioPlan.action = copy`, no encoder, hence the
published audio codec is still `opus`). The other cases are real transcodes:
VP9/Opus -> H.264/AAC (`system-smarttv-compat`), H.264 -> H.265
(`system-smarttv-hevc`), AAC -> MP3 and FLAC -> FLAC for the audio presets,
which also prove `video: remove`.

Exit codes: 64 usage, 65 verification failure (ffprobe evidence printed),
66 missing fixture, 69 missing tool or socket, 75 the daemon did not answer.
"""

from __future__ import annotations

import json
import os
import socket
import subprocess
import sys
import time
from dataclasses import dataclass
from shutil import which
from typing import NoReturn

TERMINAL_STATES = frozenset({"completed", "failed", "cancelled", "interrupted"})
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIXTURES_DIR = os.environ.get("CAUCE_FIXTURES", os.path.join(REPO_ROOT, "target", "cauce-fixtures"))
FIXTURE_GENERATOR = os.path.join(REPO_ROOT, "tests", "e2e", "fixtures.sh")
POLL_BUDGET_S = float(os.environ.get("CAUCE_E2E_TIMEOUT", "180"))
POLL_INTERVAL_S = 0.25
# v0.1 validates duration with `max(2000 ms, 3 %)` (`docs/architecture.md` §12).
DURATION_TOLERANCE_MS = 2000.0
DURATION_TOLERANCE_RATIO = 0.03


@dataclass(frozen=True)
class Case:
    """One input/preset pair and the published file it must produce."""

    preset: str
    source: str
    container: tuple[str, ...]
    video: str | None
    audio: str
    max_width: int | None = None
    max_height: int | None = None
    channels: int | None = None
    note: str = ""


# The five real conversions of T23. Codecs and limits come from
# `docs/presets.md`; nothing here is invented.
CASES = (
    Case(
        preset="system-smarttv-compat",
        source="sample-vp9-opus.webm",
        container=("mov", "mp4"),
        video="h264",
        audio="aac",
        max_width=1920,
        max_height=1080,
        channels=2,
        note="transcode: vp9+opus -> h264+aac in MP4",
    ),
    Case(
        preset="system-smarttv-hevc",
        source="sample-h264-aac.mp4",
        container=("matroska",),
        video="hevc",
        audio="aac",
        max_width=3840,
        max_height=2160,
        note="transcode: h264 -> h265 in Matroska",
    ),
    Case(
        preset="system-archival-master",
        source="sample-vp9-opus.webm",
        container=("matroska",),
        video="hevc",
        audio="opus",
        note="COPY case: Opus audio copied while the VP9 video is transcoded",
    ),
    Case(
        preset="system-audio-mp3",
        source="sample-h264-aac.mp4",
        container=("mp3",),
        video=None,
        audio="mp3",
        note="transcode to MP3 and remove the video stream",
    ),
    Case(
        preset="system-audio-flac",
        source="sample-audio.flac",
        container=("flac",),
        video=None,
        audio="flac",
        note="audio-only preset: FLAC output, no video stream",
    ),
)


def fail(code: int, message: str, evidence: object | None = None) -> NoReturn:
    """Prints the failure with its evidence and leaves with a non-zero status."""
    print("FAIL " + message, file=sys.stderr)
    if evidence is not None:
        print(json.dumps(evidence, indent=2, ensure_ascii=False, sort_keys=True), file=sys.stderr)
    raise SystemExit(code)


def ffprobe(path: str) -> dict:
    """The exact probe shape the task fixes: streams and format as JSON."""
    result = subprocess.run(
        ["ffprobe", "-v", "error", "-print_format", "json", "-show_streams",
         "-show_format", path],
        capture_output=True, text=True, check=False,
    )
    if result.returncode != 0:
        fail(65, f"ffprobe could not read {path} (exit {result.returncode})",
             {"stderr": result.stderr.strip()})
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        fail(65, f"ffprobe returned unparsable JSON for {path}: {error}", result.stdout)


def stream_of(probe: dict, kind: str) -> dict | None:
    for stream in probe.get("streams", []):
        if stream.get("codec_type") == kind:
            return stream
    return None


def duration_ms(probe: dict) -> float:
    """Format duration, falling back to the first stream that declares one."""
    candidates = [probe.get("format", {}).get("duration")]
    candidates += [stream.get("duration") for stream in probe.get("streams", [])]
    for value in candidates:
        try:
            return float(value) * 1000.0
        except (TypeError, ValueError):
            continue
    fail(65, "ffprobe reported no duration for the file", probe)


class Daemon:
    """One NDJSON JSON-RPC 2.0 connection to the Cauce socket."""

    def __init__(self, path: str) -> None:
        if not os.path.exists(path):
            fail(69, f"the daemon socket {path} does not exist; start `cauce-daemon` "
                     "first (gui_smoke.sh does it for you)")
        self.next_id = 0
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(POLL_BUDGET_S)
        try:
            self.sock.connect(path)
        except OSError as error:
            fail(69, f"could not connect to the daemon socket {path}: {error}")
        self.stream = self.sock.makefile("rwb")

    def call(self, method: str, params: dict | None = None) -> object:
        self.next_id += 1
        request_id = self.next_id
        payload = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params or {}}
        try:
            self.stream.write(json.dumps(payload).encode("utf-8") + b"\n")
            self.stream.flush()
        except OSError as error:
            fail(69, f"the daemon closed the connection while sending `{method}`: {error}")
        while True:
            try:
                line = self.stream.readline()
            except OSError as error:
                fail(75, f"no answer to `{method}` within {POLL_BUDGET_S}s: {error}")
            if not line:
                fail(69, f"the daemon closed the connection while {method!r} was pending")
            try:
                message = json.loads(line)
            except json.JSONDecodeError as error:
                fail(65, f"the daemon sent an unparsable line: {error}",
                     line.decode(errors="replace"))
            # `job.updated` / `job.progress` / `queue.updated` are notifications
            # without an `id`, so they are not the answer to this request.
            if message.get("id") != request_id:
                continue
            if "error" in message:
                fail(65, f"`{method}` failed: {message['error']}", message["error"])
            return message.get("result")

    def close(self) -> None:
        for closer in (getattr(self, "stream", None), getattr(self, "sock", None)):
            try:
                closer.close()
            except (OSError, AttributeError):
                pass


def ensure_fixtures() -> None:
    expected = sorted({case.source for case in CASES})
    missing = [name for name in expected if not os.path.isfile(os.path.join(FIXTURES_DIR, name))]
    if not missing:
        return
    if not os.path.isfile(FIXTURE_GENERATOR):
        fail(66, f"missing fixtures {missing} and no generator at {FIXTURE_GENERATOR}")
    print(f"fixtures missing ({', '.join(missing)}); running {FIXTURE_GENERATOR}", flush=True)
    result = subprocess.run(["bash", FIXTURE_GENERATOR, FIXTURES_DIR],
                            capture_output=True, text=True, check=False)
    sys.stdout.write(result.stdout)
    if result.returncode != 0:
        fail(66, f"the fixture generator failed (exit {result.returncode})", result.stderr.strip())


def verify(case: Case, output: str, source_duration: float) -> tuple[dict, float]:
    """Checks the published file against the preset and returns the evidence."""
    probe = ffprobe(output)
    names = str(probe.get("format", {}).get("format_name", "")).split(",")
    if not any(name in names for name in case.container):
        fail(65, f"{case.preset}: container is `{probe.get('format', {}).get('format_name')}`, "
                 f"expected one of {list(case.container)}", probe)

    video = stream_of(probe, "video")
    if case.video is None:
        if video is not None:
            fail(65, f"{case.preset}: the preset removes video but "
                     f"`{video.get('codec_name')}` was published", probe)
    else:
        if video is None:
            fail(65, f"{case.preset}: no video stream in {output}", probe)
        if video.get("codec_name") != case.video:
            fail(65, f"{case.preset}: video codec is `{video.get('codec_name')}`, "
                     f"expected `{case.video}`", probe)
        width, height = int(video.get("width", 0)), int(video.get("height", 0))
        if (case.max_width and width > case.max_width) or (case.max_height and height > case.max_height):
            fail(65, f"{case.preset}: {width}x{height} exceeds the preset limit "
                     f"{case.max_width}x{case.max_height}", probe)

    audio = stream_of(probe, "audio")
    if audio is None:
        fail(65, f"{case.preset}: no audio stream in {output}", probe)
    if audio.get("codec_name") != case.audio:
        fail(65, f"{case.preset}: audio codec is `{audio.get('codec_name')}`, "
                 f"expected `{case.audio}`", probe)
    # v0.1 never upmixes (`docs/presets.md`): the preset count is a ceiling.
    channels = int(audio.get("channels", 0))
    if case.channels is not None and channels > case.channels:
        fail(65, f"{case.preset}: {channels} channels exceed the preset ceiling "
                 f"of {case.channels}", probe)

    published_ms = duration_ms(probe)
    tolerance = max(DURATION_TOLERANCE_MS, source_duration * DURATION_TOLERANCE_RATIO)
    if abs(published_ms - source_duration) > tolerance:
        fail(65, f"{case.preset}: duration {published_ms:.0f}ms is off the source "
                 f"{source_duration:.0f}ms by more than {tolerance:.0f}ms", probe)
    return {"video": video or {}, "audio": audio}, published_ms


def run_case(daemon: Daemon, case: Case, output_dir: str) -> None:
    source = os.path.join(FIXTURES_DIR, case.source)
    if not os.path.isfile(source):
        fail(66, f"fixture {source} is missing; run `bash tests/e2e/fixtures.sh`")
    # The published file must never be the input, and the input must stay intact.
    before = os.stat(source)
    source_duration = duration_ms(ffprobe(source))

    jobs = daemon.call("jobs.add", {
        "inputs": [source],
        "output_dir": output_dir,
        "preset_id": case.preset,
        "duplicate_policy": "suffix",
    })
    if not isinstance(jobs, list) or len(jobs) != 1:
        fail(65, f"jobs.add returned {jobs!r}; exactly one job was expected")
    job = jobs[0]
    job_id, output = job["id"], job["output_path"]
    if output == source:
        fail(65, f"{case.preset}: the daemon planned the input path as its own output", job)

    deadline = time.monotonic() + POLL_BUDGET_S
    while job["state"] not in TERMINAL_STATES:
        if time.monotonic() >= deadline:
            fail(75, f"{case.preset}: job {job_id} stayed `{job['state']}` "
                     f"for {POLL_BUDGET_S}s", job)
        time.sleep(POLL_INTERVAL_S)
        job = daemon.call("jobs.get", {"id": job_id})
    if job["state"] != "completed":
        fail(65, f"{case.preset}: job {job_id} ended in `{job['state']}`: "
                 f"{(job.get('error') or {}).get('message')}", job)
    if not os.path.isfile(output):
        fail(65, f"{case.preset}: the job completed but {output} does not exist", job)

    after = os.stat(source)
    if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
        fail(65, f"{case.preset}: the input {source} was modified by the conversion", job)

    evidence, published_ms = verify(case, output, source_duration)
    print(f"OK {case.preset} {os.path.basename(output)} "
          f"{evidence['video'].get('codec_name', '-')}/{evidence['audio'].get('codec_name')} "
          f"{published_ms:.0f}ms", flush=True)


class Usage(Exception):
    """A command-line problem; the caller turns it into exit 64."""


def parse_args(argv: list[str]) -> tuple[str, str, str | None]:
    args = list(argv[1:])
    only = None
    if "--only" in args:
        index = args.index("--only")
        if index + 1 >= len(args):
            raise Usage("--only needs a preset id")
        only = args[index + 1]
        del args[index:index + 2]
    if len(args) != 2:
        raise Usage("expected <socket> <output-dir>")
    return args[0], args[1], only


def main(argv: list[str]) -> int:
    try:
        socket_path, output_dir, only = parse_args(argv)
    except Usage as error:
        print(f"real_conversions.py: {error}\n"
              "usage: real_conversions.py <socket> <output-dir> [--only <preset-id>]",
              file=sys.stderr)
        return 64
    if not os.path.isdir(output_dir):
        print(f"real_conversions.py: the output dir {output_dir} does not exist", file=sys.stderr)
        return 64
    for tool in ("ffprobe", "ffmpeg"):
        if which(tool) is None:
            print(f"real_conversions.py: {tool} is missing; "
                  "install it with `sudo apt install ffmpeg`", file=sys.stderr)
            return 69

    cases = [case for case in CASES if only is None or case.preset == only]
    if not cases:
        known = ", ".join(case.preset for case in CASES)
        print(f"real_conversions.py: `{only}` is not one of the cases: {known}", file=sys.stderr)
        return 64

    ensure_fixtures()
    daemon = Daemon(socket_path)
    try:
        info = daemon.call("initialize", {"client_name": "cauce-e2e", "client_version": "0.1.0"})
        if info.get("protocol_version") != 1:
            fail(65, f"unexpected protocol version in {info!r}")
        for case in cases:
            print(f"-- {case.preset}: {case.note}", flush=True)
            run_case(daemon, case, output_dir)
    finally:
        daemon.close()

    print(f"real_conversions.py: OK {len(cases)} case(s) in {output_dir}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
