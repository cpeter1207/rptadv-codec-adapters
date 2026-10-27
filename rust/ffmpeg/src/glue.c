/** @file
 * @brief Opaque FFmpeg context and S16 frame access for the Rust G.723
 * provider.
 * @copyright 2026 Chris Peterson, GPL-2.0-only.
 */
#include <libavcodec/avcodec.h>
#include <libavutil/channel_layout.h>
#include <stdlib.h>
#include <string.h>

/** One exclusively owned direction; all processing runs outside audio
 * callbacks. */
struct codec_context {
  AVCodecContext *codec;
  AVFrame *frame;
  AVPacket *packet;
};

/** Release all direction resources. The owner supplies a nonnull context. */
void rptadv_g723_close(void *opaque) {
  struct codec_context *context = opaque;
  av_packet_free(&context->packet);
  av_frame_free(&context->frame);
  avcodec_free_context(&context->codec);
  free(context);
}

/** Open the released 6.3 kbps encoder or dual-rate decoder, without
 * version3/nonfree. */
void *rptadv_g723_open(int encoder) {
  const char *configuration = avcodec_configuration();
  if (strstr(configuration, "--enable-version3") ||
      strstr(configuration, "--enable-nonfree"))
    return NULL;
  const AVCodec *codec = encoder ? avcodec_find_encoder(AV_CODEC_ID_G723_1)
                                 : avcodec_find_decoder(AV_CODEC_ID_G723_1);
  if (!codec)
    return NULL;
  struct codec_context *context = calloc(1, sizeof(*context));
  if (!context)
    return NULL;
  context->codec = avcodec_alloc_context3(codec);
  context->frame = av_frame_alloc();
  context->packet = av_packet_alloc();
  if (!context->codec || !context->frame || !context->packet)
    goto failed;
  context->codec->sample_rate = 8000;
  context->codec->sample_fmt = AV_SAMPLE_FMT_S16;
  context->codec->bit_rate = 6300;
  av_channel_layout_default(&context->codec->ch_layout, 1);
  if (avcodec_open2(context->codec, codec, NULL) < 0)
    goto failed;
  if (encoder) {
    context->frame->format = AV_SAMPLE_FMT_S16;
    context->frame->nb_samples = 240;
    av_channel_layout_default(&context->frame->ch_layout, 1);
    if (av_frame_get_buffer(context->frame, 0) < 0)
      goto failed;
  }
  return context;
failed:
  rptadv_g723_close(context);
  return NULL;
}

/** Encode exactly 240 samples into capacity for one 24-byte packet. */
int rptadv_g723_encode(void *opaque, const int16_t *pcm, uint8_t *out) {
  struct codec_context *context = opaque;
  if (av_frame_make_writable(context->frame) < 0)
    return -1;
  memcpy(context->frame->data[0], pcm, 240 * sizeof(*pcm));
  if (avcodec_send_frame(context->codec, context->frame) < 0 ||
      avcodec_receive_packet(context->codec, context->packet) < 0)
    return -1;
  int count = context->packet->size;
  if (count == 24)
    memcpy(out, context->packet->data, 24);
  av_packet_unref(context->packet);
  return count == 24 ? count : -1;
}

/** Decode one already-inspected frame, preserving native decoder concealment.
 */
int rptadv_g723_decode(void *opaque, const uint8_t *packet, int bytes,
                       int16_t *out) {
  struct codec_context *context = opaque;
  av_packet_unref(context->packet);
  if (av_new_packet(context->packet, bytes) < 0)
    return -1;
  memcpy(context->packet->data, packet, (size_t)bytes);
  if (avcodec_send_packet(context->codec, context->packet) < 0 ||
      avcodec_receive_frame(context->codec, context->frame) < 0)
    return -1;
  int count = context->frame->nb_samples;
  if (count == 240 && context->frame->format == AV_SAMPLE_FMT_S16P)
    memcpy(out, context->frame->data[0], 240 * sizeof(*out));
  else
    count = -1;
  av_frame_unref(context->frame);
  return count;
}
