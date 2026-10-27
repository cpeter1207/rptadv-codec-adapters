"""G.723 modes and independent, hash-pinned FFmpeg FATE wire packets."""

import ctypes as c
import hashlib
import os
import subprocess
import urllib.request
from pathlib import Path

from test_g711 import Config, Encoded, Packet
from test_native_profiles import ProviderHarness


class G723(ProviderHarness):
    def test_profile_g723_modes_and_fate(self):
        library, api, (encoder, decoder) = self.api("ffmpeg", 4, 240, 6300)
        config = Config(c.sizeof(Config), 4, 5300, 240, 65503)
        unsupported = c.c_void_p()
        self.assertEqual(api.open_encoder(c.byref(config), c.byref(unsupported)), 4)
        pcm = (c.c_float * 240)()
        packet = (c.c_uint8 * 24)()
        result = Encoded(struct_size=c.sizeof(Encoded))
        self.assertEqual(
            api.encode(encoder, pcm, 240, packet, 24, 0, c.byref(result)), 0
        )
        self.assertEqual((result.encoded_bytes, result.packet_frames), (24, 240))
        info = Packet(struct_size=c.sizeof(Packet))
        self.assertEqual(api.inspect(decoder, packet, 23, c.byref(info)), 3)
        for name, size, mode, expected in [
            (
                "ineqd53.tco",
                20,
                5300,
                "a8369f49e2b7ddaf5560009d64b225c0d487065cc51e47bc38b9ec65430a9bb7",
            ),
            (
                "overd63p.tco",
                24,
                6300,
                "d97ac34d1b66784966d9c5cc29786cdb24cc6b9a73d5f5b45ff648af9824fb19",
            ),
        ]:
            # Test data is fetched privately, not redistributed in source/packages.
            directory = Path(
                os.environ.get(
                    "RPTADV_CODEC_FIXTURES",
                    Path(__file__).resolve().parents[1] / ".work/conformance",
                )
            )
            directory.mkdir(parents=True, exist_ok=True)
            fixture = directory / name
            if not fixture.exists():
                with urllib.request.urlopen(
                    f"https://fate-suite.ffmpeg.org/g723_1/{name}", timeout=30
                ) as source:
                    fixture.write_bytes(source.read())
            data = fixture.read_bytes()
            self.assertEqual(hashlib.sha256(data).hexdigest(), expected)
            self.assertEqual(api.reset(decoder), 0)
            actual_pcm = bytearray()
            for start in range(0, len(data), size):
                encoded = (c.c_uint8 * size).from_buffer_copy(
                    data[start : start + size]
                )
                self.assertEqual(api.inspect(decoder, encoded, size, c.byref(info)), 0)
                self.assertEqual((info.frames, info.mode), (240, mode))
                frames = c.c_size_t()
                self.assertEqual(
                    api.decode(decoder, encoded, size, pcm, 240, c.byref(frames)), 0
                )
                self.assertEqual(frames.value, 240)
                actual_pcm.extend(
                    bytes((c.c_int16 * 240)(*(int(x * 32768) for x in pcm)))
                )
            reference = subprocess.run(
                [
                    "ffmpeg",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "g723_1",
                    "-postfilter",
                    "1",
                    "-i",
                    "pipe:0",
                    "-f",
                    "s16le",
                    "pipe:1",
                ],
                input=data,
                capture_output=True,
                check=True,
                timeout=30,
            ).stdout
            self.assertEqual(bytes(actual_pcm), reference)
        self.assertTrue(library)
