/* Stub — shadows firmware/export/load_code.h. Codecs only need
 * struct lc_header for the CODEC_HEADER macro; loading is static (the
 * shim's codec table references the renamed __header_<name> symbols
 * directly, like firmware/target/hosted/android/cdylib/lc-android.c). */
#ifndef RBCODEC_LOAD_CODE_H
#define RBCODEC_LOAD_CODE_H

struct lc_header {
    unsigned long magic;
    unsigned short target_id;
    unsigned short api_version;
    unsigned char *load_addr;
    unsigned char *end_addr;
};

#endif
