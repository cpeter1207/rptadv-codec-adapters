# Released source evidence — 2026-10-27

The adjacent manifest records selected released sources and SHA256 pins, not
implemented codec coverage. GPLv2-compatible source notices were reviewed for
the selected files. Runtime library configurations and actual link dependencies
still require native package verification. No separate patent opinion or grant
is inferred from a source license.

- G.711: select the existing algorithm crate's 0BSD alternative and retain its notice.
- GSM, Speex, Opus and iLBC: retain their permissive redistribution notices.
  libilbc 2.0.2 bundles its C signal-processing dependency without Abseil;
  its referenced PATENTS file is absent, so no separate patent grant is claimed.
- SpanDSP 0.0.6: audited G.726, G.722 and LPC10 sources state LGPL2.1. Use the
  separately shipped libspandsp.so.2. G.726 LEFT/RIGHT preserve AAL2/RFC packing;
  G.722 options zero uses 16 kHz PCM; LPC10 error correction must be enabled.
- bcg729 1.0.4 public source headers state GPL2 or later. A current GPL3-only
  bcg729 library is not an interchangeable dependency. Package the pinned
  compatible shared library separately and verify its binary identity.
- Asterisk 20.21.0's Dialogic primitive is GPL2 and matches the inspected
  Asterisk branch baseline. Only its codec primitive is selected: never its
  app_rpt code or host runtime. Keep default bit-identical arithmetic, not
  NOT_BLI/AUTO_RETURN. Reset payload 0x42 decodes to PCM16 [288,480].
- FFmpeg's G.723 implementation is LGPL2.1 or later; verify version3/nonfree
  dependencies are disabled. It encodes 6300 and decodes 5300/6300. The official
  archive endpoint timed out; the manifest pins the independently downloaded
  GitHub release-tag source, not an unverified replacement hash.

Do not use FFmpeg's G.726 encoder for live 20 ms packets: its 2048-frame unit
is 256 ms and short frames finalize it. SpanDSP's released streaming API
avoids that mismatch. SpanDSP's OKI ADPCM is not the selected exact Dialogic
implementation: payload 0x42 produces [288,464], not Asterisk's [288,480].

All downloaded external archives remain in ignored .work/sources for inspection;
they are not vendored into controller, IAX or provider source trees.

Provider evidence: native ARM64 Rust 1.85.1 tests exercise signed PCM byte order,
G.711 endpoint vectors, the pinned Asterisk GSM/Speex/G.722/G.726/LPC10 packets,
and an independently produced 60 ms libopus packet. This is initial framing and
ABI evidence, not a claim that every direction/mode has complete conformance or
coverage. Source fixtures retain Digium's GPL-2.0 attribution in their manifest.

The native FFmpeg runtime is Debian 13 `7.1.5-0+deb13u1+rpt2`, not the reference
7.1.1 archive. Its configuration disables version3 and nonfree components.
Primary-source comparison against n7.1.5 found identical G.723 common/decoder
sources and LGPL notices; the encoder's minimum-error accumulator was widened
to int64. Independent FATE `ineqd53.tco`/`overd63p.tco` decode successfully.
Keep source hashes distinct from binary identity. Public FATE test bytes are
hash-verified private downloads, not redistributed in source/packages.

External backends build from verified archives into qualified DSOs:
`librptadv_bcg729_gpl2.so.0`, `librptadv_ilbc2.so.2` and
`librptadv_dialogic.so.1`. Their upstream copyright files ship separately.
The Dialogic build extracts only the released primitive before Asterisk glue;
the one bounded negative left shift is replaced with multiplication by 16,
preserving every output while avoiding undefined C arithmetic.

Plain Asterisk 20.21.0 IAX uses attribute-free iLBC at 30 ms/50 bytes. The
translator defaults to mode 30; its SDP attribute default of 20 does not
negotiate IAX mode. Support both explicit modes without guessing aggregated
packet lengths. G.729 Annex A vectors exist in ITU's Release 3 archive;
redistribution permission was not established, so fetch them privately for
tests rather than labeling them GPL fixtures. Asterisk has no ex_g729.h.
