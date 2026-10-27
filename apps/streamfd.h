/***************************************************************************
 * streamfd.h - Unified stream I/O abstraction for local files and HTTP(S)
 *              network streams.
 *
 * In the SDL simulator build and the hosted SDL application build, URLs that
 * start with "http://" or "https://" are opened as network streams backed by
 * the Rust "netstream" crate.  All other paths are handled by the normal
 * Rockbox file-system functions.
 *
 * File-descriptor encoding (simulator / hosted SDL app):
 *   fd == -1                  : closed / unset sentinel
 *   fd >= 0                   : normal file descriptor
 *   fd <= STREAM_HTTP_FD_BASE : HTTP stream handle
 *                               handle_id = STREAM_HTTP_FD_BASE - fd
 *
 * On all other (embedded) builds, every symbol reduces to the existing
 * Rockbox file-system macro/function so there is zero overhead and zero code
 * change needed in callers.
 ***************************************************************************/

#ifndef APPS_STREAMFD_H
#define APPS_STREAMFD_H

#ifdef SIMULATOR
#define STREAM_HTTP_ENABLED
#elif defined(APPLICATION)
#define STREAM_HTTP_ENABLED
#endif

#ifdef STREAM_HTTP_ENABLED

#include <sys/types.h>
#include <fcntl.h>
#include <stdint.h>

/* Sentinel: file descriptors <= this value are HTTP stream handles. */
#define STREAM_HTTP_FD_BASE (-1000)

/** Return non-zero if @p fd refers to an open HTTP stream handle. */
static inline int stream_is_http_fd(int fd)
{
    return fd <= STREAM_HTTP_FD_BASE;
}

/**
 * Open a path.
 *
 * If @p path begins with "http://" or "https://" the request is forwarded
 * to the Rust network layer; otherwise a normal open() is performed.
 *
 * @return  A file descriptor >= 0, an HTTP handle <= STREAM_HTTP_FD_BASE,
 *          or -1 on error.
 */
int stream_open(const char *path, int flags);

/**
 * Read up to @p n bytes from @p fd into @p buf.
 * Routes to read() for real fds, rb_net_read() for HTTP fds.
 */
ssize_t stream_read(int fd, void *buf, size_t n);

/**
 * Seek within @p fd.
 * Routes to lseek() for real fds, rb_net_lseek() for HTTP fds.
 */
off_t stream_lseek(int fd, off_t off, int whence);

/**
 * Close @p fd.
 * Routes to close() for real fds, rb_net_close() for HTTP fds.
 * Silently ignores fd == -1.
 *
 * @return 0 on success, -1 on error.
 */
int stream_close(int fd);

/**
 * Return the total size of the stream associated with @p fd.
 *
 * For HTTP streams: the Content-Length if known, or a large sentinel
 * value (~2 GiB) if unknown (buffering will truncate on EOF).
 * For regular fds: delegates to ffilesize().
 *
 * @return  Size in bytes, or -1 on error.
 */
off_t stream_filesize_fd(int fd);

/**
 * Copy the normalized Content-Type associated with @p fd into @p buf.
 *
 * For HTTP streams this returns the response Content-Type without parameters.
 * For regular files this returns -1.
 *
 * @return  Full string length on success, or -1 if unknown/unavailable.
 */
ssize_t stream_content_type(int fd, char *buf, size_t n);

#else /* !STREAM_HTTP_ENABLED */

/*
 * Non-simulator / embedded builds: map every symbol straight through to
 * the native Rockbox file-system API so no code needs to change in callers.
 */
#include "file.h"
#include <unistd.h>

#define stream_is_http_fd(fd)         (0)
#define stream_open(path, flags)      open((path), (flags))
#define stream_read(fd, buf, n)       read((fd), (buf), (n))
#define stream_lseek(fd, off, whence) lseek((fd), (off), (whence))
#define stream_close(fd)              close(fd)
#define stream_filesize_fd(fd)        ffilesize(fd)
#define stream_content_type(fd, buf, n) ((ssize_t)-1)

#endif /* STREAM_HTTP_ENABLED */

#endif /* APPS_STREAMFD_H */
