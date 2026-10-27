"""Exercise the actual shared provider, including buffered encoder ownership."""

import ctypes as c
import os
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIBRARY = Path(
    os.environ.get(
        "RPTADV_CODEC_G711_LIBRARY",
        ROOT
        / "target"
        / "debug"
        / (
            "rptadv_codec_g711.dll"
            if sys.platform == "win32"
            else "librptadv_codec_g711.so"
        ),
    )
)


class Config(c.Structure):
    _fields_ = [
        ("struct_size", c.c_uint32),
        ("profile_id", c.c_uint32),
        ("mode_id", c.c_uint32),
        ("packet_frames", c.c_uint32),
        ("max_packet_bytes", c.c_size_t),
    ]


class Encoded(c.Structure):
    _fields_ = [
        ("struct_size", c.c_uint32),
        ("status", c.c_uint32),
        ("consumed_frames", c.c_size_t),
        ("encoded_bytes", c.c_size_t),
        ("source_start_frames", c.c_uint64),
        ("source_frames", c.c_uint32),
        ("packet_frames", c.c_uint32),
        ("required_capacity", c.c_size_t),
    ]


class Packet(c.Structure):
    _fields_ = [
        ("struct_size", c.c_uint32),
        ("frames", c.c_uint32),
        ("mode", c.c_uint32),
        ("required_capacity", c.c_size_t),
    ]


Count = c.CFUNCTYPE(c.c_size_t)
Profile = c.CFUNCTYPE(c.c_int32, c.c_size_t, c.c_void_p)
Open = c.CFUNCTYPE(c.c_int32, c.POINTER(Config), c.POINTER(c.c_void_p))
Inspect = c.CFUNCTYPE(c.c_int32, c.c_void_p, c.c_void_p, c.c_size_t, c.POINTER(Packet))
Encode = c.CFUNCTYPE(
    c.c_int32,
    c.c_void_p,
    c.c_void_p,
    c.c_size_t,
    c.c_void_p,
    c.c_size_t,
    c.c_uint32,
    c.POINTER(Encoded),
)
Decode = c.CFUNCTYPE(
    c.c_int32,
    c.c_void_p,
    c.c_void_p,
    c.c_size_t,
    c.c_void_p,
    c.c_size_t,
    c.POINTER(c.c_size_t),
)
Reset = c.CFUNCTYPE(c.c_int32, c.c_void_p)
Destroy = c.CFUNCTYPE(None, c.c_void_p)
Conceal = c.CFUNCTYPE(
    c.c_int32, c.c_void_p, c.c_uint32, c.c_void_p, c.c_size_t, c.POINTER(c.c_size_t)
)


class Descriptor(c.Structure):
    _fields_ = [
        ("struct_size", c.c_uint32),
        ("abi_version", c.c_uint32),
        ("capability", c.c_char * 16),
        ("profile_count", Count),
        ("profile_at", Profile),
        ("open_encoder", Open),
        ("open_decoder", Open),
        ("inspect", Inspect),
        ("encode", Encode),
        ("decode", Decode),
        ("reset", Reset),
        ("destroy", Destroy),
        ("conceal", Conceal),
    ]


class G711(unittest.TestCase):
    def test_contract_g711_counts_and_isolation(self):
        """Byte/sample confusion or shared encoder state changes these results."""
        self.assertTrue(LIBRARY.is_file(), "G.711 shared provider has not been built")
        library = c.CDLL(str(LIBRARY))
        descriptor = library.rptadv_codec_descriptor_v1
        descriptor.restype = c.POINTER(Descriptor)
        api = descriptor().contents
        self.assertEqual(api.abi_version, 1)
        self.assertEqual(api.capability, b"rptadv.codec.v1")
        self.assertEqual(api.struct_size, c.sizeof(Descriptor))
        self.assertEqual(api.profile_count(), 2)
        handles = []
        self.addCleanup(lambda: [api.destroy(handle) for handle in handles])

        def open_context(profile, encoder=True):
            config = Config(c.sizeof(Config), profile, 0, 160, 65503)
            handle = c.c_void_p()
            self.assertEqual(
                (api.open_encoder if encoder else api.open_decoder)(
                    c.byref(config), c.byref(handle)
                ),
                0,
            )
            self.assertTrue(handle.value)
            handles.append(handle)
            return handle

        def encode(handle, samples, capacity=160, flush=False):
            pcm = (c.c_float * len(samples))(*samples)
            output = (c.c_uint8 * capacity)(*([0xA5] * capacity))
            result = Encoded(struct_size=c.sizeof(Encoded))
            status = api.encode(
                handle, pcm, len(samples), output, capacity, flush, c.byref(result)
            )
            self.assertEqual(result.status, status)
            return status, bytes(output), result

        first, second = open_context(1), open_context(1)
        status, _, result = encode(first, [0.0] * 37)
        self.assertEqual(
            (status, result.consumed_frames, result.encoded_bytes), (1, 37, 0)
        )
        status, output, result = encode(second, [1.0] * 160)
        self.assertEqual(
            (status, result.encoded_bytes, result.packet_frames), (0, 160, 160)
        )
        self.assertEqual(output, b"\x80" * 160)
        status, output, result = encode(first, [0.0] * 123, capacity=159)
        self.assertEqual(
            (status, result.consumed_frames, result.required_capacity), (2, 0, 160)
        )
        self.assertEqual(output, b"\xa5" * 159)
        status, output, result = encode(first, [0.0] * 123)
        self.assertEqual(
            (status, result.source_start_frames, result.source_frames), (0, 0, 160)
        )
        self.assertEqual(output, b"\xff" * 160)
        self.assertEqual(api.reset(first), 0)
        status, output, result = encode(first, [-1.0, 0.0, 1.0], flush=True)
        self.assertEqual(
            (status, result.source_start_frames, result.source_frames), (0, 0, 3)
        )
        self.assertEqual(output[:3], b"\x00\xff\x80")
        status, _, result = encode(first, [], flush=True)
        self.assertEqual(
            (status, result.encoded_bytes, result.source_frames), (0, 0, 0)
        )

        decoder = open_context(1, False)
        encoded = (c.c_uint8 * 4)(0x00, 0x80, 0xFF, 0x7F)
        packet = Packet(struct_size=c.sizeof(Packet))
        self.assertEqual(api.inspect(decoder, encoded, 4, c.byref(packet)), 0)
        self.assertEqual(packet.frames, 4)
        pcm = (c.c_float * 4)()
        frames = c.c_size_t()
        self.assertEqual(api.decode(decoder, encoded, 4, pcm, 4, c.byref(frames)), 0)
        self.assertEqual(list(pcm), [-32124 / 32768, 32124 / 32768, 0.0, 0.0])
        self.assertEqual(frames.value, 4)

        alaw = open_context(2, False)
        alaw_encoded = (c.c_uint8 * 2)(0xD5, 0x55)
        self.assertEqual(api.decode(alaw, alaw_encoded, 2, pcm, 4, c.byref(frames)), 0)
        self.assertEqual(list(pcm)[:2], [8 / 32768, -8 / 32768])

    def test_contract_rejects_short_config(self):
        """A caller with an old/short layout cannot create a context."""
        self.assertTrue(LIBRARY.is_file(), "G.711 shared provider has not been built")
        library = c.CDLL(str(LIBRARY))
        library.rptadv_codec_descriptor_v1.restype = c.POINTER(Descriptor)
        api = library.rptadv_codec_descriptor_v1().contents
        handle = c.c_void_p()
        config = Config(4, 1, 0, 160, 65503)
        self.assertEqual(api.open_encoder(c.byref(config), c.byref(handle)), 3)
        self.assertFalse(handle.value)


if __name__ == "__main__":
    unittest.main()
