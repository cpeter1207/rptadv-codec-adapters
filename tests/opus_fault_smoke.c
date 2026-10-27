/** @file
 * @brief Inject native Opus failures only into this test executable's DSO.
 * Real signal conformance and PLC use the unmodified released library.
 */
#include "rptadv_codec.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <opus/opus.h>
#include <stdarg.h>

static void *native;
static int fault, calls, live;

OpusEncoder *opus_encoder_create(opus_int32 rate, int channels, int application,
                                 int *error) {
  OpusEncoder *(*create)(opus_int32, int, int, int *) =
      dlsym(native, "opus_encoder_create");
  if (fault == 1) {
    ++calls;
    return NULL;
  }
  OpusEncoder *state = create(rate, channels, application, error);
  assert(state);
  ++live;
  if (fault == 2) {
    ++calls;
    *error = OPUS_INTERNAL_ERROR;
  }
  return state;
}

OpusDecoder *opus_decoder_create(opus_int32 rate, int channels, int *error) {
  OpusDecoder *(*create)(opus_int32, int, int *) =
      dlsym(native, "opus_decoder_create");
  if (fault == 1) {
    ++calls;
    return NULL;
  }
  OpusDecoder *state = create(rate, channels, error);
  assert(state);
  ++live;
  if (fault == 2) {
    ++calls;
    *error = OPUS_INTERNAL_ERROR;
  }
  return state;
}

void opus_encoder_destroy(OpusEncoder *state) {
  void (*destroy)(OpusEncoder *) = dlsym(native, "opus_encoder_destroy");
  --live;
  destroy(state);
}

void opus_decoder_destroy(OpusDecoder *state) {
  void (*destroy)(OpusDecoder *) = dlsym(native, "opus_decoder_destroy");
  --live;
  destroy(state);
}

int opus_encoder_ctl(OpusEncoder *state, int request, ...) {
  (void)state;
  int result = OPUS_OK;
  va_list arguments;
  va_start(arguments, request);
  if (request == OPUS_SET_DTX_REQUEST) {
    int enabled = va_arg(arguments, int);
    assert(enabled == 0);
    if (fault == 3)
      result = OPUS_INTERNAL_ERROR;
  } else if (request == OPUS_GET_LOOKAHEAD_REQUEST) {
    opus_int32 *value = va_arg(arguments, opus_int32 *);
    *value = fault == 5 ? -1 : 312;
    if (fault == 4)
      result = OPUS_INTERNAL_ERROR;
  } else {
    assert(request == OPUS_RESET_STATE);
    if (fault == 6)
      result = OPUS_INTERNAL_ERROR;
  }
  va_end(arguments);
  ++calls;
  return result;
}

int opus_decoder_ctl(OpusDecoder *state, int request, ...) {
  (void)state;
  assert(request == OPUS_RESET_STATE);
  ++calls;
  return fault == 6 ? OPUS_INTERNAL_ERROR : OPUS_OK;
}

opus_int32 opus_encode_float(OpusEncoder *state, const float *pcm, int frames,
                             unsigned char *packet, opus_int32 maximum) {
  (void)state;
  (void)pcm;
  (void)frames;
  (void)packet;
  (void)maximum;
  ++calls;
  return OPUS_INTERNAL_ERROR;
}

int opus_decode_float(OpusDecoder *state, const unsigned char *packet,
                      opus_int32 bytes, float *pcm, int maximum, int fec) {
  (void)state;
  (void)bytes;
  (void)pcm;
  (void)fec;
  ++calls;
  return packet ? OPUS_INVALID_PACKET : maximum - 1;
}

int main(int argc, char **argv) {
  assert(argc == 2);
  native = dlopen("libopus.so.0", RTLD_NOW | RTLD_LOCAL);
  assert(native);
  void *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
  assert(library);
  const rptadv_codec_descriptor_v1_table *(*descriptor)(void) =
      dlsym(library, "rptadv_codec_descriptor_v1");
  assert(descriptor);
  const rptadv_codec_descriptor_v1_table *api = descriptor();
  rptadv_codec_config_v1 config = {sizeof(config), RPTADV_CODEC_OPUS_MONO, 0,
                                   960, 65503};
  void *invalid_context = NULL;
  config.packet_frames = 480;
  assert(api->open_encoder(&config, &invalid_context) ==
         RPTADV_CODEC_MALFORMED);
  config.packet_frames = 960;
  config.max_packet_bytes = (size_t)INT_MAX + 1;
  assert(api->open_encoder(&config, &invalid_context) ==
         RPTADV_CODEC_MALFORMED);
  config.max_packet_bytes = 65503;
  for (fault = 1; fault <= 5; ++fault) {
    void *context = (void *)1;
    calls = 0;
    assert(api->open_encoder(&config, &context) == RPTADV_CODEC_FAILED);
    assert(!context && live == 0 && calls > 0);
    if (fault <= 2) {
      calls = 0;
      assert(api->open_decoder(&config, &context) == RPTADV_CODEC_FAILED);
      assert(!context && live == 0 && calls > 0);
    }
  }
  fault = 0;
  void *encoder = NULL, *decoder = NULL;
  assert(api->open_encoder(&config, &encoder) == RPTADV_CODEC_COMPLETE);
  assert(api->open_decoder(&config, &decoder) == RPTADV_CODEC_COMPLETE);
  float pcm[6000] = {0};
  uint8_t packet[1275] = {0};
  uint8_t invalid_packet = 0xff;
  rptadv_codec_packet_info_v1 info = {.struct_size = sizeof(info)};
  assert(api->inspect(decoder, &invalid_packet, 1, &info) ==
         RPTADV_CODEC_MALFORMED);
  rptadv_codec_encode_result_v1 result = {.struct_size = sizeof(result)};
  calls = 0;
  assert(api->encode(encoder, pcm, 960, packet, sizeof(packet), 0, &result) ==
         RPTADV_CODEC_FAILED);
  assert(calls == 1 && !result.encoded_bytes);
  size_t frames = 123;
  calls = 0;
  assert(api->decode(decoder, packet, 1, pcm, 960, &frames) ==
         RPTADV_CODEC_MALFORMED);
  assert(calls == 1 && frames == 0);
  calls = 0;
  assert(api->conceal(decoder, 960, pcm, 960, &frames) == RPTADV_CODEC_FAILED);
  assert(calls == 1 && frames == 0);
  for (uint32_t invalid = 1; invalid <= 6000; invalid += 5999)
    assert(api->conceal(decoder, invalid, pcm, 6000, &frames) ==
           RPTADV_CODEC_UNSUPPORTED);
  fault = 6;
  calls = 0;
  assert(api->reset(encoder) == RPTADV_CODEC_FAILED);
  assert(api->reset(decoder) == RPTADV_CODEC_FAILED);
  assert(calls == 2);
  api->destroy(encoder);
  api->destroy(decoder);
  assert(live == 0);
  assert(dlclose(library) == 0);
  assert(dlclose(native) == 0);
  return 0;
}
