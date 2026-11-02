/** @file
 * @brief Host-free shared wrapper around the hash-pinned Asterisk Dialogic
 * primitive.
 * @copyright 2026 Chris Peterson; GPL-2.0-only. Upstream notice retained at
 * build.
 */
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "released_adpcm.inc"

/** Allocate an independent zero-history encoder or decoder. */
void *rptadv_dialogic_open(void) {
  return calloc(1, sizeof(struct adpcm_state));
}
/** Release exclusively owned state. */
void rptadv_dialogic_close(void *state) { free(state); }
/** Clear history at an explicit stream discontinuity. */
void rptadv_dialogic_reset(void *state) {
  memset(state, 0, sizeof(struct adpcm_state));
}
/** Encode complete two-sample units, high nibble first, into validated storage.
 */
void rptadv_dialogic_encode(void *state, const int16_t *pcm, uint8_t *out,
                            size_t frames) {
  for (size_t i = 0; i < frames; i += 2) {
    int high = adpcm(pcm[i], state);
    int low = adpcm(pcm[i + 1], state);
    out[i / 2] = (uint8_t)((high << 4) | low);
  }
}
/** Decode complete bytes into two samples each, preserving the released state.
 */
void rptadv_dialogic_decode(void *state, const uint8_t *packet, int16_t *out,
                            size_t bytes) {
  for (size_t i = 0; i < bytes; ++i) {
    out[i * 2] = decode(packet[i] >> 4, state);
    out[i * 2 + 1] = decode(packet[i] & 15, state);
  }
}
