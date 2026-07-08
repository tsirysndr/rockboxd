/***************************************************************************
 *             __________               __   ___.
 *   Open      \______   \ ____   ____ |  | _\_ |__   _______  ___
 *   Source     |       _//  _ \_/ ___\|  |/ /| __ \ /  _ \  \/  /
 *   Jukebox    |    |   (  <_> )  \___|    < | \_\ (  <_> > <  <
 *   Firmware   |____|_  /\____/ \___  >__|_ \|___  /\____/__/\_ \
 *                     \/            \/     \/    \/            \/
 * $Id$
 *
 * Copyright (C) 2005 Dave Chapman
 *
 * This program is free software; you can redistribute it and/or
 * modify it under the terms of the GNU General Public License
 * as published by the Free Software Foundation; either version 2
 * of the License, or (at your option) any later version.
 *
 * This software is distributed on an "AS IS" basis, WITHOUT WARRANTY OF ANY
 * KIND, either express or implied.
 *
 ****************************************************************************/
#include <inttypes.h>
#include "metadata.h"

/* In simulator / hosted-SDL-app builds, file descriptors for HTTP(S) streams
 * are encoded as integers <= STREAM_HTTP_FD_BASE (-1000).  Plain POSIX
 * read() / lseek() / filesize() do not handle those values.  Route all
 * metadata I/O through the stream abstraction so that HTTP handles work
 * identically to normal file descriptors.
 *
 * streamfd.h is in apps/, which is on the include path via -I$(APPSDIR) in
 * uisimulator.make.  The macros below are intentionally placed before the
 * read_uint* macros so those expansions are also redirected. */
#if defined(SIMULATOR) || defined(APPLICATION)
#include "streamfd.h"
/* Route read() and lseek() through the stream abstraction so that HTTP(S)
 * file descriptors (encoded as integers <= STREAM_HTTP_FD_BASE = -1000) are
 * handled correctly by all metadata parsers.
 *
 * filesize() is intentionally NOT redefined here.  It is also a member name
 * of struct mp3entry (expanded via FS_PREFIX in firmware/include/file.h), so
 * redefining it as a function-like macro would break every `id3->filesize`
 * struct access with a "no member named 'filesize'" compiler error.
 *
 * Instead, metadata parsers that need a correct file size (e.g. for CBR
 * duration estimation) must call metadata_filesize(fd), defined below.
 * stream_filesize_fd() returns Content-Length for HTTP fds and delegates to
 * the platform filesize() for regular fds. */
#ifdef read
#undef read
#endif
#define read(fd, buf, n)       stream_read((fd),  (buf), (n))
#ifdef lseek
#undef lseek
#endif
#define lseek(fd, off, whence) stream_lseek((fd), (off), (whence))
#define metadata_filesize(fd)  stream_filesize_fd((fd))
#else
#define metadata_filesize(fd)  filesize(fd)
#endif /* SIMULATOR || APPLICATION */

#ifdef ROCKBOX_BIG_ENDIAN
#define IS_BIG_ENDIAN 1
#else
#define IS_BIG_ENDIAN 0
#endif

#define TAG_NAME_LENGTH             32
#define TAG_VALUE_LENGTH            128

#define FOURCC(a,b,c,d) ((((unsigned long)(a)) << 24) | (((unsigned long)(b)) << 16) | \
                         (((unsigned long)(c)) <<  8) | ((unsigned long)(d)))

enum tagtype { TAGTYPE_APE = 1, TAGTYPE_VORBIS };

bool read_ape_tags(int fd, struct mp3entry* id3);
long read_vorbis_tags(int fd, struct mp3entry *id3,
    long tag_remaining);

struct ogg_file
{
    int fd;
    bool packet_ended;
    long packet_remaining;
};

#ifdef HAVE_ALBUMART
int id3_unsynchronize(char* tag, int len, bool *ff_found);

size_t base64_decode(const char *in, size_t in_len, unsigned char *out);

bool parse_flac_album_art(unsigned char *buf, int bytes_read, enum mp3_aa_type *type, int *picframe_pos);

int get_ogg_format_and_move_to_comments(int fd, unsigned char *buf);
bool ogg_file_init(struct ogg_file* file, int fd, int type, int remaining);
ssize_t ogg_file_read(struct ogg_file* file, void* buffer, size_t buffer_size);
#endif

int string_option(const char *option, const char *const oplist[], bool ignore_case);
bool skip_id3v2(int fd, struct mp3entry *id3);
long read_string(int fd, char* buf, long buf_size, int eos, long size);

int read_uint8(int fd, uint8_t* buf);
#ifdef ROCKBOX_BIG_ENDIAN
#define read_uint16be(fd,buf) read((fd), (buf), 2)
#define read_uint32be(fd,buf) read((fd), (buf), 4)
#define read_uint64be(fd,buf) read((fd), (buf), 8)
int read_uint16le(int fd, uint16_t* buf);
int read_uint32le(int fd, uint32_t* buf);
int read_uint64le(int fd, uint64_t* buf);
#else
int read_uint16be(int fd, uint16_t* buf);
int read_uint32be(int fd, uint32_t* buf);
int read_uint64be(int fd, uint64_t* buf);
#define read_uint16le(fd,buf) read((fd), (buf), 2)
#define read_uint32le(fd,buf) read((fd), (buf), 4)
#define read_uint64le(fd,buf) read((fd), (buf), 8)
#endif

uint64_t get_uint64_le(void* buf);
uint32_t get_long_le(void* buf);
uint16_t get_short_le(void* buf);
uint32_t get_long_be(void* buf);
uint16_t get_short_be(void* buf);
int32_t get_slong(void* buf);
uint32_t get_itunes_int32(char* value, int count);
long parse_tag(const char* name, char* value, struct mp3entry* id3,
    char* buf, long buf_remaining, enum tagtype type);
