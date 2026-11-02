"""Compare provider signal output with separate released-library contexts."""

import ctypes as c
import math
import subprocess

from test_g711 import Encoded
from test_native_profiles import PROFILES, ProviderHarness

P = c.c_void_p
I = c.c_int


def function(library, name, result, *arguments):
    """Bind the released C API, independently of the provider's Rust bindings."""
    call = getattr(library, name)
    call.restype = result
    call.argtypes = list(arguments)
    return call


class Bits(c.Structure):
    """Public SpeexBits layout from released speex_bits.h."""

    _fields_ = [
        ("chars", P),
        ("nbBits", I),
        ("charPtr", I),
        ("bitPtr", I),
        ("owner", I),
        ("overflow", I),
        ("buf_size", I),
        ("reserved1", I),
        ("reserved2", P),
    ]


def reference(test, backend, profile, frames, mode):
    """Return encoder and decoder calls using independent native states."""
    if backend in ("gsm", "spandsp"):
        library = c.CDLL("libgsm.so.1" if backend == "gsm" else "libspandsp.so.2")
        if backend == "gsm":
            create = function(library, "gsm_create", P)
            close = function(library, "gsm_destroy", None, P)
            encoder, decoder = create(), create()
            encode = function(library, "gsm_encode", I, P, P, P)
            decode = function(library, "gsm_decode", I, P, P, P)

            def encode_pcm(pcm, packet):
                encode(encoder, pcm, packet)
                return 33

            def decode_packet(packet, count, pcm):
                test.assertEqual(decode(decoder, packet, pcm), 0)
                return 160

        else:
            name = (
                "g726" if profile in (5, 12) else "g722" if profile == 13 else "lpc10"
            )
            encode = function(library, name + "_encode", I, P, P, P, I)
            decode = function(library, name + "_decode", I, P, P, P, I)
            if name == "g726":
                create = function(library, "g726_init", P, P, I, I, I)
                encoder = create(None, 32000, 0, 1 if profile == 5 else 2)
                decoder = create(None, 32000, 0, 1 if profile == 5 else 2)
                close = close_decoder = function(library, "g726_free", I, P)
            else:
                args = (64000, 0) if name == "g722" else (1,)
                signature = [P] + [I] * len(args)
                encoder = function(library, name + "_encode_init", P, *signature)(
                    None, *args
                )
                decoder = function(library, name + "_decode_init", P, *signature)(
                    None, *args
                )
                close = function(library, name + "_encode_free", I, P)
                close_decoder = function(library, name + "_decode_free", I, P)

            def encode_pcm(pcm, packet):
                return encode(encoder, packet, pcm, frames)

            def decode_packet(packet, count, pcm):
                return decode(decoder, pcm, packet, count)

        test.assertTrue(encoder and decoder)
        test.addCleanup(close, encoder)
        test.addCleanup(close if backend == "gsm" else close_decoder, decoder)
        return encode_pcm, decode_packet, False

    if backend == "bcg729":
        library = c.CDLL("librptadv_bcg729_gpl2.so.0")
        encoder = function(library, "initBcg729EncoderChannel", P, c.c_uint8)(0)
        decoder = function(library, "initBcg729DecoderChannel", P)()
        test.addCleanup(
            function(library, "closeBcg729EncoderChannel", None, P), encoder
        )
        test.addCleanup(
            function(library, "closeBcg729DecoderChannel", None, P), decoder
        )
        encode = function(library, "bcg729Encoder", None, P, P, P, P)
        decode = function(library, "bcg729Decoder", None, P, P, *([c.c_uint8] * 4), P)

        def encode_pcm(pcm, packet):
            count = c.c_uint8()
            encode(encoder, pcm, packet, c.byref(count))
            return count.value

        def decode_packet(packet, count, pcm):
            decode(decoder, packet, count, 0, int(count == 2), 0, pcm)
            return 80

        return encode_pcm, decode_packet, False

    if backend == "ilbc":
        library = c.CDLL("librptadv_ilbc2.so.2")
        handles = []
        for direction in ("Encoder", "Decoder"):
            handle = P()
            test.assertEqual(
                function(library, f"WebRtcIlbcfix_{direction}Create", c.c_int16, P)(
                    c.byref(handle)
                ),
                0,
            )
            test.addCleanup(
                function(library, f"WebRtcIlbcfix_{direction}Free", c.c_int16, P),
                handle,
            )
            test.assertEqual(
                function(
                    library, f"WebRtcIlbcfix_{direction}Init", c.c_int16, P, c.c_int16
                )(handle, mode),
                0,
            )
            handles.append(handle)
        encode = function(
            library, "WebRtcIlbcfix_Encode", c.c_int16, P, P, c.c_int16, P
        )
        decode = function(
            library, "WebRtcIlbcfix_Decode", c.c_int16, P, P, c.c_int16, P, P
        )

        def encode_pcm(pcm, packet):
            return encode(handles[0], pcm, frames, packet)

        def decode_packet(packet, count, pcm):
            speech = c.c_int16()
            return decode(handles[1], packet, count, pcm, c.byref(speech))

        return encode_pcm, decode_packet, False

    if backend == "opus":
        library = c.CDLL("libopus.so.0")
        error = I()
        encoder = function(library, "opus_encoder_create", P, I, I, I, P)(
            48000, 1, 2048, c.byref(error)
        )
        test.assertEqual(error.value, 0)
        decoder = function(library, "opus_decoder_create", P, I, I, P)(
            48000, 1, c.byref(error)
        )
        test.assertEqual(error.value, 0)
        test.addCleanup(function(library, "opus_encoder_destroy", None, P), encoder)
        test.addCleanup(function(library, "opus_decoder_destroy", None, P), decoder)
        test.assertEqual(
            function(library, "opus_encoder_ctl", I, P, I, I)(encoder, 4016, 0), 0
        )
        encode = function(library, "opus_encode_float", I, P, P, I, P, c.c_int32)
        decode = function(library, "opus_decode_float", I, P, P, I, P, I, I)

        def encode_pcm(pcm, packet):
            return encode(encoder, pcm, frames, packet, 1275)

        def decode_packet(packet, count, pcm):
            return decode(decoder, packet, count, pcm, frames, 0)

        return encode_pcm, decode_packet, True

    test.assertEqual(backend, "speex")
    library = c.CDLL("libspeex.so.1")
    native_mode = function(library, "speex_lib_get_mode", P, I)(int(profile == 15))
    encoder = function(library, "speex_encoder_init", P, P)(native_mode)
    decoder = function(library, "speex_decoder_init", P, P)(native_mode)
    test.addCleanup(function(library, "speex_encoder_destroy", None, P), encoder)
    test.addCleanup(function(library, "speex_decoder_destroy", None, P), decoder)
    control = function(library, "speex_encoder_ctl", I, P, I, P)
    for request, value in ((4, 3), (12, 0), (30, 0), (34, 0)):
        test.assertEqual(control(encoder, request, c.byref(I(value))), 0)
    bits_out, bits_in = Bits(), Bits()
    for bits in (bits_out, bits_in):
        function(library, "speex_bits_init", None, P)(c.byref(bits))
        test.addCleanup(function(library, "speex_bits_destroy", None, P), c.byref(bits))
    reset = function(library, "speex_bits_reset", None, P)
    pack = function(library, "speex_bits_pack", None, P, I, I)
    write = function(library, "speex_bits_write", I, P, P, I)
    read = function(library, "speex_bits_read_from", None, P, P, I)
    encode = function(library, "speex_encode_int", I, P, P, P)
    decode = function(library, "speex_decode_int", I, P, P, P)

    def encode_pcm(pcm, packet):
        reset(c.byref(bits_out))
        test.assertGreaterEqual(encode(encoder, pcm, c.byref(bits_out)), 0)
        pack(c.byref(bits_out), 15, 5)
        return write(c.byref(bits_out), packet, 4096)

    def decode_packet(packet, count, pcm):
        read(c.byref(bits_in), packet, count)
        test.assertEqual(decode(decoder, c.byref(bits_in), pcm), 0)
        return frames

    return encode_pcm, decode_packet, False


class ReleasedReference(ProviderHarness):
    def test_released_encode_and_decode_signal_output(self):
        packing = {}
        for backend, profile, frames, _, mode, _ in PROFILES:
            if backend in ("g711", "pcm", "dialogic", "ffmpeg"):
                continue  # Independent boundary/FATE fixtures live in existing tests.
            with self.subTest(profile=profile, mode=mode):
                _, api, (encoder, decoder) = self.api(backend, profile, frames, mode)
                encode, decode, floating = reference(
                    self, backend, profile, frames, mode
                )
                for block in range(6):
                    integers = [
                        int(20000 * math.sin((i + block * frames) * 0.071))
                        for i in range(frames)
                    ]
                    samples = (c.c_float * frames)(*(x / 32768 for x in integers))
                    native = samples if floating else (c.c_int16 * frames)(*integers)
                    expected = (c.c_int16 * 2048)()  # Aligned iLBC word storage.
                    size = encode(native, expected)
                    self.assertGreater(size, 0)
                    actual = (c.c_uint8 * 4096)()
                    result = Encoded(struct_size=c.sizeof(Encoded))
                    self.assertEqual(
                        api.encode(
                            encoder, samples, frames, actual, 4096, 0, c.byref(result)
                        ),
                        0,
                    )
                    self.assertEqual(
                        bytes(actual[: result.encoded_bytes]), bytes(expected)[:size]
                    )
                    if profile in (5, 12):
                        packing[profile] = bytes(actual[: result.encoded_bytes])
                    expected_pcm = (
                        (c.c_float * frames)() if floating else (c.c_int16 * frames)()
                    )
                    self.assertEqual(decode(expected, size, expected_pcm), frames)
                    actual_pcm = (c.c_float * frames)()
                    count = c.c_size_t()
                    self.assertEqual(
                        api.decode(
                            decoder, expected, size, actual_pcm, frames, c.byref(count)
                        ),
                        0,
                    )
                    self.assertEqual(count.value, frames)
                    values = (
                        list(expected_pcm)
                        if floating
                        else [x / 32768 for x in expected_pcm]
                    )
                    self.assertEqual(list(actual_pcm), values)
        self.assertNotEqual(packing[5], packing[12])
        self.assertEqual(
            packing[5], bytes((b >> 4) | ((b & 15) << 4) for b in packing[12])
        )

    def test_g723_matches_ffmpeg_command_output(self):
        """Use the separate released command, not provider glue, as the oracle."""
        _, api, (encoder, decoder) = self.api("ffmpeg", 4, 240, 6300)
        integers = [int(20000 * math.sin(i * 0.071)) for i in range(240 * 6)]
        linear = (c.c_int16 * len(integers))(*integers)
        common = ["ffmpeg", "-hide_banner", "-loglevel", "error"]
        wire = subprocess.run(
            common
            + [
                "-f",
                "s16le",
                "-ar",
                "8000",
                "-ac",
                "1",
                "-i",
                "pipe:0",
                "-c:a",
                "g723_1",
                "-b:a",
                "6300",
                "-f",
                "g723_1",
                "pipe:1",
            ],
            input=bytes(linear),
            capture_output=True,
            check=True,
            timeout=30,
        ).stdout
        encoded = bytearray()
        for start in range(0, len(integers), 240):
            pcm = (c.c_float * 240)(*(x / 32768 for x in integers[start : start + 240]))
            packet = (c.c_uint8 * 24)()
            result = Encoded(struct_size=c.sizeof(Encoded))
            self.assertEqual(
                api.encode(encoder, pcm, 240, packet, 24, 0, c.byref(result)), 0
            )
            encoded.extend(packet[: result.encoded_bytes])
        self.assertEqual(bytes(encoded), wire)
        expected = subprocess.run(
            common + ["-f", "g723_1", "-i", "pipe:0", "-f", "s16le", "pipe:1"],
            input=wire,
            capture_output=True,
            check=True,
            timeout=30,
        ).stdout
        actual = bytearray()
        for start in range(0, len(wire), 24):
            packet = (c.c_uint8 * 24).from_buffer_copy(wire[start : start + 24])
            pcm = (c.c_float * 240)()
            count = c.c_size_t()
            self.assertEqual(
                api.decode(decoder, packet, 24, pcm, 240, c.byref(count)), 0
            )
            actual.extend(
                bytes((c.c_int16 * count.value)(*(int(x * 32768) for x in pcm)))
            )
        self.assertEqual(bytes(actual), expected)
