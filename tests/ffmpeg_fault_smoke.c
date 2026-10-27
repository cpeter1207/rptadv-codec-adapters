/** @file
 * @brief Exercise the actual FFmpeg struct-access glue with native failures.
 * Linker wrapping affects this executable only, not the released provider.
 */
#include <assert.h>
#include <libavcodec/avcodec.h>
#include <stdlib.h>
#include <string.h>

void *rptadv_g723_open(int encoder);
void rptadv_g723_close(void *state);
int rptadv_g723_encode(void *state, const int16_t *pcm, uint8_t *packet);
int rptadv_g723_decode(void *state, const uint8_t *packet, int bytes,
                       int16_t *pcm);

static int fault, hits;
static int fails(int selected) {
  if (fault != selected)
    return 0;
  ++hits;
  return 1;
}

const char *__wrap_avcodec_configuration(void) {
  if (fails(1))
    return "--enable-version3";
  if (fails(2))
    return "--enable-nonfree";
  return "";
}

const AVCodec *__real_avcodec_find_encoder(enum AVCodecID id);
const AVCodec *__wrap_avcodec_find_encoder(enum AVCodecID id) {
  return fails(3) ? NULL : __real_avcodec_find_encoder(id);
}
const AVCodec *__real_avcodec_find_decoder(enum AVCodecID id);
const AVCodec *__wrap_avcodec_find_decoder(enum AVCodecID id) {
  return fails(3) ? NULL : __real_avcodec_find_decoder(id);
}
void *__real_calloc(size_t count, size_t size);
void *__wrap_calloc(size_t count, size_t size) {
  return fails(4) ? NULL : __real_calloc(count, size);
}
AVCodecContext *__real_avcodec_alloc_context3(const AVCodec *codec);
AVCodecContext *__wrap_avcodec_alloc_context3(const AVCodec *codec) {
  return fails(5) ? NULL : __real_avcodec_alloc_context3(codec);
}
AVFrame *__real_av_frame_alloc(void);
AVFrame *__wrap_av_frame_alloc(void) {
  return fails(6) ? NULL : __real_av_frame_alloc();
}
AVPacket *__real_av_packet_alloc(void);
AVPacket *__wrap_av_packet_alloc(void) {
  return fails(7) ? NULL : __real_av_packet_alloc();
}
int __real_avcodec_open2(AVCodecContext *, const AVCodec *, AVDictionary **);
int __wrap_avcodec_open2(AVCodecContext *context, const AVCodec *codec,
                         AVDictionary **options) {
  return fails(8) ? -1 : __real_avcodec_open2(context, codec, options);
}
int __real_av_frame_get_buffer(AVFrame *, int);
int __wrap_av_frame_get_buffer(AVFrame *frame, int alignment) {
  return fails(9) ? -1 : __real_av_frame_get_buffer(frame, alignment);
}
int __real_av_frame_make_writable(AVFrame *);
int __wrap_av_frame_make_writable(AVFrame *frame) {
  return fails(10) ? -1 : __real_av_frame_make_writable(frame);
}
int __real_avcodec_send_frame(AVCodecContext *, const AVFrame *);
int __wrap_avcodec_send_frame(AVCodecContext *context, const AVFrame *frame) {
  return fails(11) ? -1 : __real_avcodec_send_frame(context, frame);
}
int __real_avcodec_receive_packet(AVCodecContext *, AVPacket *);
int __wrap_avcodec_receive_packet(AVCodecContext *context, AVPacket *packet) {
  if (fails(12))
    return -1;
  int status = __real_avcodec_receive_packet(context, packet);
  if (fails(13))
    packet->size = 23;
  return status;
}
int __real_av_new_packet(AVPacket *, int);
int __wrap_av_new_packet(AVPacket *packet, int bytes) {
  return fails(14) ? -1 : __real_av_new_packet(packet, bytes);
}
int __real_avcodec_send_packet(AVCodecContext *, const AVPacket *);
int __wrap_avcodec_send_packet(AVCodecContext *context,
                               const AVPacket *packet) {
  return fails(15) ? -1 : __real_avcodec_send_packet(context, packet);
}
int __real_avcodec_receive_frame(AVCodecContext *, AVFrame *);
int __wrap_avcodec_receive_frame(AVCodecContext *context, AVFrame *frame) {
  if (fails(16))
    return -1;
  int status = __real_avcodec_receive_frame(context, frame);
  if (fails(17))
    frame->nb_samples = 239;
  if (fails(18))
    frame->format = AV_SAMPLE_FMT_S16;
  return status;
}

int main(void) {
  for (fault = 1; fault <= 9; ++fault) {
    for (int encoder = fault == 9; encoder <= 1; ++encoder) {
      hits = 0;
      const void *context = rptadv_g723_open(encoder);
      assert(!context && hits > 0);
    }
  }
  fault = 0;
  const int16_t pcm[240] = {0};
  uint8_t packet[24] = {0};
  void *encoder = rptadv_g723_open(1);
  assert(encoder);
  int count = rptadv_g723_encode(encoder, pcm, packet);
  assert(count == 24);
  rptadv_g723_close(encoder);
  for (int selected = 10; selected <= 18; ++selected) {
    fault = 0;
    void *context = rptadv_g723_open(selected <= 13);
    assert(context);
    fault = selected;
    hits = 0;
    if (selected <= 13) {
      uint8_t untouched[24];
      memset(untouched, 0x55, sizeof(untouched));
      count = rptadv_g723_encode(context, pcm, untouched);
      for (size_t i = 0; i < sizeof(untouched); ++i)
        assert(untouched[i] == 0x55);
    } else {
      int16_t untouched[240];
      memset(untouched, 0x55, sizeof(untouched));
      count = rptadv_g723_decode(context, packet, 24, untouched);
      for (size_t i = 0; i < 240; ++i)
        assert(untouched[i] == 0x5555);
    }
    assert(count == -1 && hits > 0);
    rptadv_g723_close(context);
  }
  return 0;
}
