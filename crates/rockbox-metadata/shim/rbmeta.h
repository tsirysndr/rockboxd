/* rbmeta.h — flat C bridge between Rockbox's struct mp3entry and Rust.
 *
 * struct mp3entry stores its strings as pointers into internal scratch
 * buffers whose layout depends on config macros; rbmeta_read() copies
 * everything into this fixed-layout struct so the Rust side can mirror it
 * with #[repr(C)] without tracking firmware ifdefs. */
#ifndef RBMETA_H
#define RBMETA_H

#include <stdint.h>

#define RBMETA_STR      512
#define RBMETA_COMMENT  1024
#define RBMETA_GENRE    256
#define RBMETA_SMALL    64
#define RBMETA_CODEC    32

struct rbmeta_tags {
    char codec[RBMETA_CODEC];        /* format label, e.g. "MP3", "FLAC" */
    char title[RBMETA_STR];
    char artist[RBMETA_STR];
    char album[RBMETA_STR];
    char albumartist[RBMETA_STR];
    char composer[RBMETA_STR];
    char grouping[RBMETA_STR];
    char comment[RBMETA_COMMENT];
    char genre[RBMETA_GENRE];
    char year_string[RBMETA_SMALL];
    char track_string[RBMETA_SMALL];
    char disc_string[RBMETA_SMALL];
    char mb_track_id[RBMETA_SMALL];  /* MusicBrainz track id */

    int32_t codectype;               /* AFMT_* index */
    int32_t tracknum;                /* -1 / 0 = unset */
    int32_t discnum;
    int32_t year;
    int32_t layer;                   /* MPEG layer */
    int32_t id3version;              /* ID3_VER_* */
    int32_t vbr;                     /* bool */
    int32_t bitrate;                 /* kbit/s */

    uint32_t frequency;              /* Hz */
    uint32_t reserved_;              /* keep 8-byte alignment */

    uint64_t filesize;               /* audio payload bytes (no tag headers) */
    uint64_t length_ms;              /* track length in milliseconds */
    uint64_t samples;                /* total PCM frames (0 if unknown) */
    uint64_t frame_count;            /* MPEG frames if VBR (0 if unknown) */
    uint64_t first_frame_offset;     /* byte offset of first audio frame */

    /* ReplayGain. 0 = tag not present.
     *   *_level : gain in dB as Q19.12 (dB × 4096)
     *   *_gain  : linear scale factor as Q7.24 — feed directly to
     *             rockbox-dsp's set_replaygain_gains_raw()
     *   *_peak  : linear peak amplitude as Q7.24 (1.0 = full scale)  */
    int64_t track_level;
    int64_t album_level;
    int64_t track_gain;
    int64_t album_gain;
    int64_t track_peak;
    int64_t album_peak;

    /* Embedded album art (ID3 APIC / FLAC PICTURE / MP4 covr).
     * type low nibble: 0 unknown, 1 BMP, 2 PNG, 3 JPEG;
     * flag bits 4/5: ID3-unsynchronized / Vorbis-base64 (see metadata.h). */
    int32_t has_albumart;
    int32_t albumart_type;
    int64_t albumart_pos;
    int32_t albumart_size;

    /* Embedded cuesheet. encoding: 1 latin-1, 2 utf-8, 3 utf-16le, 4 utf-16be */
    int32_t has_cuesheet;
    int64_t cuesheet_pos;
    int32_t cuesheet_size;
    int32_t cuesheet_encoding;
};

/* Parse the file at `path` into `out`.
 * Returns 0 on success, -1 if the file can't be opened, -2 if no parser
 * recognized the file, -3 on allocation failure.
 * NOT thread-safe (some Rockbox parsers use static state) — the Rust
 * wrapper serializes calls behind a mutex. */
int rbmeta_read(const char *path, struct rbmeta_tags *out);

/* Format label for an AFMT_* index ("MP3", "FLAC", …), or NULL if out of
 * range. */
const char *rbmeta_codec_label(unsigned int codectype);

#endif /* RBMETA_H */
