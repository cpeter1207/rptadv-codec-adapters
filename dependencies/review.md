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
