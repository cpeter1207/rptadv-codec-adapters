"""Independent wire-endian fixtures for the two signed-linear profiles."""

import ctypes as c
import os
import sys
import unittest
from pathlib import Path

from test_g711 import Config, Descriptor, Encoded, Packet

ROOT = Path(__file__).resolve().parents[1]
LIBRARY = Path(
    os.environ.get(
        "RPTADV_CODEC_PCM_LIBRARY",
        ROOT
        / "target/debug"
        / (
            "rptadv_codec_pcm.dll"
            if sys.platform == "win32"
            else "librptadv_codec_pcm.so"
        ),
    )
)


class Pcm(unittest.TestCase):
    def test_profile_vectors_and_bounds(self):
        self.assertTrue(LIBRARY.is_file(), "PCM provider has not been built")
        library = c.CDLL(str(LIBRARY))
        library.rptadv_codec_descriptor_v1.restype = c.POINTER(Descriptor)
        api = library.rptadv_codec_descriptor_v1().contents
        for profile, fixture in [(7, b"\x12\x34\xff\xff"), (14, b"\x34\x12\xff\xff")]:
            with self.subTest(profile=profile):
                config = Config(c.sizeof(Config), profile, 0, 2, 65503)
                encoder, decoder = c.c_void_p(), c.c_void_p()
                self.assertEqual(api.open_encoder(c.byref(config), c.byref(encoder)), 0)
                self.assertEqual(api.open_decoder(c.byref(config), c.byref(decoder)), 0)
                self.addCleanup(api.destroy, encoder)
                self.addCleanup(api.destroy, decoder)
                pcm = (c.c_float * 2)(0x1234 / 32768, -1 / 32768)
                encoded = (c.c_uint8 * 4)()
                result = Encoded(struct_size=c.sizeof(Encoded))
                self.assertEqual(
                    api.encode(encoder, pcm, 2, encoded, 4, 0, c.byref(result)), 0
                )
                self.assertEqual(
                    (
                        result.consumed_frames,
                        result.encoded_bytes,
                        result.packet_frames,
                    ),
                    (2, 4, 2),
                )
                self.assertEqual(bytes(encoded), fixture)
                packet = Packet(struct_size=c.sizeof(Packet))
                self.assertEqual(api.inspect(decoder, encoded, 3, c.byref(packet)), 3)
                self.assertEqual(api.inspect(decoder, encoded, 4, c.byref(packet)), 0)
                self.assertEqual(packet.frames, 2)
                decoded = (c.c_float * 2)(0.5, 0.5)
                count = c.c_size_t()
                self.assertEqual(
                    api.decode(decoder, encoded, 4, decoded, 1, c.byref(count)), 2
                )
                self.assertEqual(list(decoded), [0.5, 0.5])
                self.assertEqual(
                    api.decode(decoder, encoded, 4, decoded, 2, c.byref(count)), 0
                )
                self.assertEqual(list(decoded), list(pcm))
