/** @file
 * @brief Exercise the public codec header against a real provider.
 */
#include "rptadv_codec.h"
#include <assert.h>
#include <string.h>

int main(void) {
  const rptadv_codec_descriptor_v1_table *api = rptadv_codec_descriptor_v1();
  rptadv_codec_config_v1 config = {sizeof(config), RPTADV_CODEC_G711_MU, 0, 160,
                                   65503};
  rptadv_codec_profile_v1 profile = {0};
  rptadv_codec_encode_result_v1 result = {0};
  float pcm[160] = {0};
  uint8_t encoded[160];
  void *encoder = NULL;
  size_t i;

  assert(api != NULL && api->struct_size == sizeof(*api));
  assert(api->abi_version == RPTADV_CODEC_ABI_VERSION);
  assert(memcmp(api->capability, RPTADV_CODEC_CAPABILITY,
                sizeof(api->capability)) == 0);
  profile.struct_size = sizeof(profile);
  assert(api->profile_at(0, &profile) == RPTADV_CODEC_COMPLETE);
  assert(profile.id == RPTADV_CODEC_G711_MU && profile.sample_rate_hz == 8000);
  assert(profile.channels == 1 && profile.mode_count == 1);
  assert(profile.modes[0].frame_samples == 1);
  assert(api->open_encoder(&config, &encoder) == RPTADV_CODEC_COMPLETE);
  result.struct_size = sizeof(result);
  assert(api->encode(encoder, pcm, 160, encoded, sizeof(encoded), 0, &result) ==
         RPTADV_CODEC_COMPLETE);
  assert(result.consumed_frames == 160 && result.encoded_bytes == 160);
  assert(result.source_frames == 160 && result.packet_frames == 160);
  for (i = 0; i < sizeof(encoded); ++i) {
    assert(encoded[i] == 0xff);
  }
  api->destroy(encoder);
  return 0;
}
