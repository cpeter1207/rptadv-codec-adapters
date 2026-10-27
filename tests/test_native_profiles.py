"""Real library comparisons plus pinned ASL wire fixtures, without hardware."""

import ctypes as c
import json
import math
import os
import unittest
from pathlib import Path

from test_g711 import Config, Descriptor, Encoded, Packet

DIRECTORY = Path(
    os.environ.get(
        "RPTADV_CODEC_DIR", Path(__file__).resolve().parents[1] / "target/debug"
    )
)


class ProviderHarness(unittest.TestCase):
    def api(self, backend, profile, frames, mode=0):
        library = c.CDLL(str(DIRECTORY / f"librptadv_codec_{backend}.so"))
        library.rptadv_codec_descriptor_v1.restype = c.POINTER(Descriptor)
        api = library.rptadv_codec_descriptor_v1().contents
        handles = []
        config = Config(c.sizeof(Config), profile, mode, frames, 65503)
        for open_context in (api.open_encoder, api.open_decoder):
            handle = c.c_void_p()
            self.assertEqual(open_context(c.byref(config), c.byref(handle)), 0)
            self.addCleanup(api.destroy, handle)
            handles.append(handle)
        return library, api, handles


class NativeProfiles(ProviderHarness):
    def test_malformed_opus_is_input_error_not_provider_failure(self):
        _, api, (_, decoder) = self.api("opus", 16, 960)
        # Valid duration header, truncated VBR size field: libopus rejects the
        # packet body instead of concealing it implicitly.
        packet = (c.c_uint8 * 2)(0x02, 0xFF)
        info = Packet(struct_size=c.sizeof(Packet))
        self.assertEqual(api.inspect(decoder, packet, 2, c.byref(info)), 0)
        self.assertEqual(info.frames, 960)
        pcm = (c.c_float * 960)()
        count = c.c_size_t()
        self.assertEqual(api.decode(decoder, packet, 2, pcm, 960, c.byref(count)), 3)
        self.assertEqual(count.value, 0)
        self.assertEqual(api.conceal(decoder, 960, pcm, 960, c.byref(count)), 0)
        self.assertEqual(count.value, 960)

    def test_native_plc_precedes_shared_ring_fallback(self):
        for backend, profile, frames, mode in [
            ("opus", 16, 960, 0),
            ("speex", 10, 160, 0),
            ("speex", 15, 320, 0),
            ("ilbc", 11, 160, 20),
            ("ilbc", 11, 240, 30),
            ("bcg729", 9, 80, 0),
            ("g711", 1, 160, 0),
        ]:
            with self.subTest(backend=backend, profile=profile, mode=mode):
                _, api, (encoder, decoder) = self.api(backend, profile, frames, mode)
                self.assertGreaterEqual(api.struct_size, c.sizeof(Descriptor))
                pcm = (c.c_float * frames)(
                    *(0.2 * math.sin(i * 0.13) for i in range(frames))
                )
                packet = (c.c_uint8 * 4000)()
                result = Encoded(struct_size=c.sizeof(Encoded))
                count = c.c_size_t()
                output = (c.c_float * frames)()
                for _ in range(4):
                    self.assertEqual(
                        api.encode(
                            encoder,
                            pcm,
                            frames,
                            packet,
                            len(packet),
                            0,
                            c.byref(result),
                        ),
                        0,
                    )
                    self.assertEqual(
                        api.decode(
                            decoder,
                            packet,
                            result.encoded_bytes,
                            output,
                            frames,
                            c.byref(count),
                        ),
                        0,
                    )
                output[:] = [0.75] * frames
                self.assertEqual(
                    api.conceal(decoder, frames, output, frames - 1, c.byref(count)), 2
                )
                self.assertEqual(count.value, frames)
                self.assertEqual(list(output), [0.75] * frames)
                status = api.conceal(decoder, frames, output, frames, c.byref(count))
                if backend == "g711":
                    self.assertEqual((status, count.value), (4, 0))
                    self.assertEqual(list(output), [0.75] * frames)
                else:
                    self.assertEqual((status, count.value), (0, frames))
                    self.assertTrue(all(-1.0 <= x <= 1.0 for x in output))
                    self.assertNotEqual(list(output), [0.75] * frames)
                    self.assertGreater(max(abs(x) for x in output), 0.00001)

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

    def test_profile_dialogic_exact_asterisk_arithmetic(self):
        library, api, (encoder, decoder) = self.api("dialogic", 6, 2)
        packet = (c.c_uint8 * 1)(0x42)
        pcm = (c.c_float * 2)()
        count = c.c_size_t()
        self.assertEqual(api.decode(decoder, packet, 1, pcm, 2, c.byref(count)), 0)
        self.assertEqual(list(pcm), [288 / 32768, 480 / 32768])
        output = (c.c_uint8 * 1)()
        result = Encoded(struct_size=c.sizeof(Encoded))
        self.assertEqual(api.encode(encoder, pcm, 2, output, 1, 0, c.byref(result)), 0)
        self.assertEqual(bytes(output), b"\x42")
        self.assertTrue(library)

    def test_profile_g729_units_and_sid(self):
        library, api, (encoder, decoder) = self.api("bcg729", 9, 80)
        pcm = (c.c_float * 80)()
        packet = (c.c_uint8 * 10)()
        result = Encoded(struct_size=c.sizeof(Encoded))
        self.assertEqual(
            api.encode(encoder, pcm, 80, packet, 10, 0, c.byref(result)), 0
        )
        self.assertEqual((result.encoded_bytes, result.packet_frames), (10, 80))
        info = Packet(struct_size=c.sizeof(Packet))
        for length, expected in [(10, 80), (2, 80), (0, 0)]:
            self.assertEqual(api.inspect(decoder, packet, length, c.byref(info)), 0)
            self.assertEqual(info.frames, expected)
        self.assertEqual(api.inspect(decoder, packet, 9, c.byref(info)), 3)
        count = c.c_size_t()
        self.assertEqual(api.decode(decoder, packet, 10, pcm, 80, c.byref(count)), 0)
        self.assertTrue(library)

    def test_profile_ilbc_explicit_modes(self):
        for mode, frames, size in [(20, 160, 38), (30, 240, 50)]:
            with self.subTest(mode=mode):
                library, api, (encoder, decoder) = self.api("ilbc", 11, frames, mode)
                pcm = (c.c_float * frames)()
                packet = (c.c_uint8 * size)()
                result = Encoded(struct_size=c.sizeof(Encoded))
                self.assertEqual(
                    api.encode(encoder, pcm, frames, packet, size, 0, c.byref(result)),
                    0,
                )
                self.assertEqual(
                    (result.encoded_bytes, result.packet_frames), (size, frames)
                )
                info = Packet(struct_size=c.sizeof(Packet))
                self.assertEqual(
                    api.inspect(decoder, packet, size - 1, c.byref(info)), 3
                )
                self.assertEqual(api.inspect(decoder, packet, size, c.byref(info)), 0)
                self.assertEqual((info.frames, info.mode), (frames, mode))
                count = c.c_size_t()
                self.assertEqual(
                    api.decode(decoder, packet, size, pcm, frames, c.byref(count)), 0
                )
                self.assertTrue(library)

    def test_ilbc_native_concealment_remains_enabled(self):
        for mode, frames, size, index, shift, mask, invalid in [
            (20, 160, 38, 2, 2, 3, [0]),
            (30, 240, 50, 5, 5, 7, [0, 6, 7]),
        ]:
            with self.subTest(mode=mode):
                _, api, (encoder, decoder) = self.api("ilbc", 11, frames, mode)
                pcm = (c.c_float * frames)()
                packet = (c.c_uint8 * size)()
                result = Encoded(struct_size=c.sizeof(Encoded))
                self.assertEqual(
                    api.encode(encoder, pcm, frames, packet, size, 0, c.byref(result)),
                    0,
                )
                bad_frames = []
                for start in invalid:
                    damaged = bytearray(packet)
                    damaged[index] = (damaged[index] & ~(mask << shift)) | (
                        start << shift
                    )
                    bad_frames.append(damaged)
                damaged = bytearray(packet)
                damaged[-1] |= 1
                bad_frames.append(damaged)
                for damaged in bad_frames:
                    for payload in (damaged, bytes(packet) + damaged):
                        encoded = (c.c_uint8 * len(payload)).from_buffer_copy(payload)
                        info = Packet(struct_size=c.sizeof(Packet))
                        self.assertEqual(
                            api.inspect(decoder, encoded, len(payload), c.byref(info)),
                            0,
                        )
                        self.assertEqual(info.frames, len(payload) // size * frames)
                        output = (c.c_float * (frames * 2))(*([0.75] * (frames * 2)))
                        count = c.c_size_t(999)
                        self.assertEqual(
                            api.decode(
                                decoder,
                                encoded,
                                len(payload),
                                output,
                                len(output),
                                c.byref(count),
                            ),
                            0,
                        )
                        self.assertEqual(count.value, len(payload) // size * frames)
                        self.assertTrue(
                            all(-1.0 <= x <= 1.0 for x in output[: count.value])
                        )

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
