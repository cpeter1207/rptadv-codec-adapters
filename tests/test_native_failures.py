"""Native ABI fault injection stays in test executables, never provider DSOs."""

import os
import shlex
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_native_profiles import DIRECTORY

ROOT = Path(__file__).resolve().parents[1]


class NativeFailures(unittest.TestCase):
    def test_ffmpeg_struct_glue_failures(self):
        """Instrument the production C source, with dependency errors at link time."""
        target = subprocess.check_output(["rustc", "-vV"], text=True)
        host = next(
            line[6:] for line in target.splitlines() if line.startswith("host: ")
        )
        flags = os.environ.get(
            "CFLAGS_" + host.replace("-", "_"), os.environ.get("CFLAGS", "")
        )
        libraries = shlex.split(
            subprocess.check_output(
                ["pkg-config", "--cflags", "--libs", "libavcodec", "libavutil"],
                text=True,
            )
        )
        wrapped = [
            "avcodec_configuration",
            "avcodec_find_encoder",
            "avcodec_find_decoder",
            "calloc",
            "avcodec_alloc_context3",
            "av_frame_alloc",
            "av_packet_alloc",
            "avcodec_open2",
            "av_frame_get_buffer",
            "av_frame_make_writable",
            "avcodec_send_frame",
            "avcodec_receive_packet",
            "av_new_packet",
            "avcodec_send_packet",
            "avcodec_receive_frame",
        ]
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as directory:
            executable = Path(directory) / "ffmpeg-fault-smoke"
            subprocess.run(
                [
                    *shlex.split(os.environ.get("CC", "cc")),
                    "-std=c11",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    *shlex.split(flags),
                    str(ROOT / "rust/ffmpeg/src/glue.c"),
                    str(ROOT / "tests/ffmpeg_fault_smoke.c"),
                    *["-Wl,--wrap=" + symbol for symbol in wrapped],
                    *libraries,
                    "-o",
                    str(executable),
                ],
                check=True,
            )
            subprocess.run([str(executable)], check=True)

    def test_opus_dependency_returns(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as directory:
            executable = Path(directory) / "opus-fault-smoke"
            subprocess.run(
                [
                    *shlex.split(os.environ.get("CC", "cc")),
                    "-std=c11",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-I" + str(ROOT / "include"),
                    str(ROOT / "tests/opus_fault_smoke.c"),
                    "-Wl,--export-dynamic",
                    "-ldl",
                    "-o",
                    str(executable),
                ],
                check=True,
            )
            subprocess.run(
                [str(executable), str(DIRECTORY / "librptadv_codec_opus.so")],
                check=True,
            )
