/** @file rptadv_codec.h
 * @brief Protocol-neutral source-rate PCM codec provider ABI.
 *
 * All operations run outside real-time callbacks. Each context has one owner.
 * Retain the provider DSO through context destruction. Borrowed buffers must be
 * valid, aligned, nonoverlapping and live through the synchronous call. A
 * record pointer always references at least its readable uint32_t size prefix;
 * complete records are required. A null buffer is valid only with a zero
 * element count. Libraries preserve their backend's native concealment and
 * expose it for lost spans; unsupported spans use the caller's fallback PLC.
 * Libraries do not resample, negotiate protocols or access sockets.
 */
#ifndef RPTADV_CODEC_H
#define RPTADV_CODEC_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Exact descriptor version. */
#define RPTADV_CODEC_ABI_VERSION 1U
/** NUL-padded descriptor capability, in a 16-byte field. */
#define RPTADV_CODEC_CAPABILITY "rptadv.codec.v1"

/** Numeric operation results, including nonfatal encoder progress. */
enum rptadv_codec_status {
  RPTADV_CODEC_COMPLETE = 0,   /**< Operation completed. */
  RPTADV_CODEC_NEED_INPUT = 1, /**< Accepted input, no complete packet yet. */
  RPTADV_CODEC_OUTPUT_FULL =
      2, /**< No input/state consumed; output is too small. */
  RPTADV_CODEC_MALFORMED = 3,   /**< Invalid record, count or payload. */
  RPTADV_CODEC_UNSUPPORTED = 4, /**< Profile, mode or direction not supplied. */
  RPTADV_CODEC_FAILED = 5 /**< Backend initialization/processing failed. */
};

/** Neutral profile IDs; never use these as transport wire-format bits. */
enum rptadv_codec_profile_id {
  RPTADV_CODEC_G711_MU = 1,        /**< G.711 mu-law. */
  RPTADV_CODEC_G711_A = 2,         /**< G.711 A-law. */
  RPTADV_CODEC_GSM_FR = 3,         /**< GSM full rate, not WAV49. */
  RPTADV_CODEC_G723 = 4,           /**< Declared G.723.1 modes. */
  RPTADV_CODEC_G726_AAL2_32 = 5,   /**< G.726 32 kb/s, high nibble first. */
  RPTADV_CODEC_DIALOGIC_ADPCM = 6, /**< Asterisk-compatible Dialogic ADPCM. */
  RPTADV_CODEC_PCM16_BE_8K = 7,    /**< Big-endian PCM16 at 8 kHz. */
  RPTADV_CODEC_LPC10 = 8,          /**< LPC10, 180 samples per frame. */
  RPTADV_CODEC_G729A = 9,          /**< Declared G.729A modes. */
  RPTADV_CODEC_SPEEX_NB = 10,      /**< Raw narrowband Speex. */
  RPTADV_CODEC_ILBC = 11,          /**< Explicit iLBC 20/30 ms modes. */
  RPTADV_CODEC_G726_RFC_32 = 12,   /**< G.726 32 kb/s, low nibble first. */
  RPTADV_CODEC_G722_64 = 13,       /**< G.722 64 kb/s, 16 kHz PCM. */
  RPTADV_CODEC_PCM16_LE_16K = 14,  /**< Little-endian PCM16 at 16 kHz. */
  RPTADV_CODEC_SPEEX_WB = 15,      /**< Raw wideband Speex. */
  RPTADV_CODEC_OPUS_MONO = 16      /**< Raw Opus, mono 48 kHz output. */
};

/** Immutable profile-local mode record. */
typedef struct rptadv_codec_mode_v1 {
  uint32_t struct_size;   /**< Complete record size. */
  uint32_t id;            /**< Profile-local mode ID. */
  uint32_t bitrate_bps;   /**< Encoded speech bits per second. */
  uint32_t frame_samples; /**< PCM frames per coding unit. */
  uint32_t frame_bytes;   /**< Encoded bytes, or zero for variable length. */
} rptadv_codec_mode_v1;

/** Immutable neutral profile; modes remain owned by the provider. */
typedef struct rptadv_codec_profile_v1 {
  uint32_t struct_size;              /**< Caller-supplied complete size. */
  uint32_t id;                       /**< Neutral profile ID. */
  uint32_t sample_rate_hz;           /**< Actual source PCM rate. */
  uint32_t channels;                 /**< PCM channel count, initially one. */
  uint32_t directions;               /**< Encode=1, decode=2. */
  uint32_t lookahead_frames;         /**< Codec lookahead at source rate. */
  uint32_t maximum_frame_samples;    /**< Maximum coding-unit PCM frames. */
  uint32_t maximum_frame_bytes;      /**< Maximum coding-unit bytes. */
  size_t mode_count;                 /**< Number of immutable modes. */
  const rptadv_codec_mode_v1 *modes; /**< Borrowed modes. */
} rptadv_codec_profile_v1;

/** Copied parameters for one independent encoder or decoder. */
typedef struct rptadv_codec_config_v1 {
  uint32_t struct_size;    /**< Caller-supplied complete size. */
  uint32_t profile_id;     /**< Neutral profile ID. */
  uint32_t mode_id;        /**< Explicit profile-local mode. */
  uint32_t packet_frames;  /**< Desired packet source PCM frames. */
  size_t max_packet_bytes; /**< Caller-derived encoded-packet bound. */
} rptadv_codec_config_v1;

/** Non-consuming complete-packet inspection result. */
typedef struct rptadv_codec_packet_info_v1 {
  uint32_t struct_size;     /**< Caller-supplied complete size. */
  uint32_t frames;          /**< Decoded source-rate PCM frames. */
  uint32_t mode;            /**< Validated profile-local mode. */
  size_t required_capacity; /**< Required capacity on output-full. */
} rptadv_codec_packet_info_v1;

/** One encoder step. At most one packet is returned; zero output is not voice.
 */
typedef struct rptadv_codec_encode_result_v1 {
  uint32_t struct_size;         /**< Caller-supplied complete size. */
  uint32_t status;              /**< Numeric rptadv_codec_status. */
  size_t consumed_frames;       /**< Input frames consumed by this call. */
  size_t encoded_bytes;         /**< Encoded bytes produced. */
  uint64_t source_start_frames; /**< First real frame since encoder reset. */
  uint32_t source_frames;       /**< Real source span excluding tail padding. */
  uint32_t packet_frames;       /**< Decoded wire span including padding. */
  size_t required_capacity;     /**< Required bytes on output-full. */
} rptadv_codec_encode_result_v1;

/** Complete immutable provider table. Non-null operations are required. */
typedef struct rptadv_codec_descriptor_v1_table {
  uint32_t struct_size;          /**< Complete table size. */
  uint32_t abi_version;          /**< Exact RPTADV_CODEC_ABI_VERSION. */
  uint8_t capability[16];        /**< Exact NUL-padded capability. */
  size_t (*profile_count)(void); /**< Number of profiles. */
  int32_t (*profile_at)(size_t,
                        rptadv_codec_profile_v1 *); /**< Inspect by index. */
  int32_t (*open_encoder)(const rptadv_codec_config_v1 *,
                          void **); /**< Independent encoder. */
  int32_t (*open_decoder)(const rptadv_codec_config_v1 *,
                          void **); /**< Independent decoder. */
  int32_t (*inspect)(void *, const uint8_t *, size_t,
                     rptadv_codec_packet_info_v1 *); /**< Packet span. */
  int32_t (*encode)(
      void *, const float *, size_t, uint8_t *, size_t, uint32_t,
      rptadv_codec_encode_result_v1 *); /**< Bounded encoder step. */
  int32_t (*decode)(void *, const uint8_t *, size_t, float *, size_t,
                    size_t *); /**< Complete packet decode. */
  int32_t (*reset)(void *);    /**< Clear codec history and source position. */
  void (*destroy)(void *);     /**< Exclusive cleanup; null is harmless. */
  int32_t (*conceal)(
      void *, uint32_t, float *, size_t,
      size_t *); /**< Native PLC for requested source frames.
                  * Unsupported returns zero frames, without
                  * changing history or output. Output-full
                  * returns required capacity without mutation. */
} rptadv_codec_descriptor_v1_table;

/** Entry point: provider DSO and contexts must outlive all borrowed table use.
 */
const rptadv_codec_descriptor_v1_table *rptadv_codec_descriptor_v1(void);

#ifdef __cplusplus
}
#endif
#endif
