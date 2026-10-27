"""Real library comparisons plus pinned ASL wire fixtures, without hardware."""

import ctypes as c
import json
import os
import unittest
from pathlib import Path

from test_g711 import Config, Descriptor, Encoded, Packet

DIRECTORY = Path(
    os.environ.get(
        "RPTADV_CODEC_DIR", Path(__file__).resolve().parents[1] / "target/debug"
    )
)


class NativeProfiles(unittest.TestCase):
    def test_profile_released_peer_fixtures(self):
        fixtures = json.loads(
            (Path(__file__).parent / "fixtures/asl_frames.json").read_text()
        )
        for fixture in fixtures:
            if fixture["backend"] == "gsm":
                continue
            with self.subTest(profile=fixture["profile"]):
                library, api, (encoder, decoder) = self.api(
                    fixture["backend"], fixture["profile"], fixture["frames"]
                )
                packet = bytes.fromhex(fixture["encoded_hex"])
                encoded = (c.c_uint8 * len(packet)).from_buffer_copy(packet)
                info = Packet(struct_size=c.sizeof(Packet))
                self.assertEqual(
                    api.inspect(decoder, encoded, len(packet), c.byref(info)), 0
                )
                self.assertEqual(info.frames, fixture["frames"])
                decoded = (c.c_float * fixture["frames"])()
                count = c.c_size_t()
                self.assertEqual(
                    api.decode(
                        decoder,
                        encoded,
                        len(packet),
                        decoded,
                        len(decoded),
                        c.byref(count),
                    ),
                    0,
                )
                output = (c.c_uint8 * 4096)()
                result = Encoded(struct_size=c.sizeof(Encoded))
                self.assertEqual(
                    api.encode(
                        encoder,
                        decoded,
                        len(decoded),
                        output,
                        len(output),
                        0,
                        c.byref(result),
                    ),
                    0,
                )
                self.assertEqual(result.packet_frames, fixture["frames"])
                self.assertGreater(result.encoded_bytes, 0)
                self.assertEqual(
                    api.inspect(decoder, output, result.encoded_bytes, c.byref(info)), 0
                )
                self.assertEqual(info.frames, fixture["frames"])
                self.assertTrue(library)

    def api(self, backend, profile, frames):
        library = c.CDLL(str(DIRECTORY / f"librptadv_codec_{backend}.so"))
        library.rptadv_codec_descriptor_v1.restype = c.POINTER(Descriptor)
        api = library.rptadv_codec_descriptor_v1().contents
        handles = []
        config = Config(c.sizeof(Config), profile, 0, frames, 65503)
        for open_context in (api.open_encoder, api.open_decoder):
            handle = c.c_void_p()
            self.assertEqual(open_context(c.byref(config), c.byref(handle)), 0)
            self.addCleanup(api.destroy, handle)
            handles.append(handle)
        return library, api, handles

    def test_profile_gsm_peer_fixture(self):
        # Asterisk 20.21.0 codecs/ex_gsm.h (GPL-2.0, Digium 2008), not WAV49.
        fixture = bytes.fromhex(
            "daa6ac2da3500049249249245040492492372452004924924724508046e36db8dc"
        )
        library, api, (encoder, decoder) = self.api("gsm", 3, 160)
        encoded = (c.c_uint8 * len(fixture)).from_buffer_copy(fixture)
        info = Packet(struct_size=c.sizeof(Packet))
        self.assertEqual(api.inspect(decoder, encoded, 33, c.byref(info)), 0)
        self.assertEqual(info.frames, 160)
        self.assertEqual(api.inspect(decoder, encoded, 32, c.byref(info)), 3)
        reference = c.CDLL("libgsm.so.1")
        reference.gsm_create.restype = c.c_void_p
        reference.gsm_decode.argtypes = [c.c_void_p, c.c_void_p, c.c_void_p]
        reference.gsm_destroy.argtypes = [c.c_void_p]
        state = reference.gsm_create()
        self.addCleanup(reference.gsm_destroy, state)
        expected = (c.c_int16 * 160)()
        self.assertEqual(reference.gsm_decode(state, encoded, expected), 0)
        decoded = (c.c_float * 160)()
        count = c.c_size_t()
        self.assertEqual(
            api.decode(decoder, encoded, 33, decoded, 160, c.byref(count)), 0
        )
        self.assertEqual(list(decoded), [sample / 32768 for sample in expected])
        output = (c.c_uint8 * 33)()
        result = Encoded(struct_size=c.sizeof(Encoded))
        self.assertEqual(
            api.encode(encoder, decoded, 160, output, 33, 0, c.byref(result)), 0
        )
        self.assertEqual((result.encoded_bytes, result.packet_frames), (33, 160))
        self.assertEqual(output[0] >> 4, 0xD)
        self.assertTrue(library)

    def test_profile_opus_received_duration(self):
        library, api, (_, decoder) = self.api("opus", 16, 960)
        reference = c.CDLL("libopus.so.0")
        reference.opus_encoder_create.restype = c.c_void_p
        reference.opus_encode_float.argtypes = [
            c.c_void_p,
            c.c_void_p,
            c.c_int,
            c.c_void_p,
            c.c_int32,
        ]
        reference.opus_encoder_destroy.argtypes = [c.c_void_p]
        error = c.c_int()
        encoder = reference.opus_encoder_create(48000, 1, 2048, c.byref(error))
        self.assertEqual(error.value, 0)
        self.addCleanup(reference.opus_encoder_destroy, encoder)
        pcm = (c.c_float * 2880)()
        packet = (c.c_uint8 * 4000)()
        length = reference.opus_encode_float(encoder, pcm, 2880, packet, 4000)
        self.assertGreater(length, 0)
        info = Packet(struct_size=c.sizeof(Packet))
        self.assertEqual(api.inspect(decoder, packet, length, c.byref(info)), 0)
        self.assertEqual(info.frames, 2880)
        decoded = (c.c_float * 2880)()
        count = c.c_size_t()
        self.assertEqual(
            api.decode(decoder, packet, length, decoded, 2879, c.byref(count)), 2
        )
        self.assertEqual(count.value, 2880)
        self.assertEqual(
            api.decode(decoder, packet, length, decoded, 2880, c.byref(count)), 0
        )
        self.assertEqual(count.value, 2880)
        self.assertTrue(library)
