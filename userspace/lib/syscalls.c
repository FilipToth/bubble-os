/* newlib porting layer for bubble-os.
 *
 * newlib configured with --disable-newlib-supplied-syscalls leaves the `_*`
 * functions below undefined and expects the port to provide them. The shapes
 * here follow libgloss/libnosys, which is the reference implementation of the
 * same set.
 *
 * The kernel side of every call is documented in .claude/specs/syscall-apis.md,
 * which is the source of truth for numbers, argument order, struct layouts and
 * error values. Anything changed here has to be changed there too.
 *
 * Deliberately does not include <unistd.h> or <fcntl.h>. Those declare the
 * same `_*` symbols with slightly different prototypes depending on newlib's
 * configuration, and a mismatch is a compile error rather than something that
 * degrades gracefully. libnosys avoids them for the same reason.
 */

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <stdlib.h>
#include <string.h>
#include <sys/times.h>
#include <sys/types.h>
#include <time.h>

#undef errno
extern int errno;

/* ------------------------------------------------------------------------
 * Syscall numbers
 * ---------------------------------------------------------------------- */

#define SYS_EXIT 1
#define SYS_WRITE 2
#define SYS_READ 3
#define SYS_EXECUTE 4
#define SYS_YIELD 5
#define SYS_WAIT_FOR_PROCESS 6
#define SYS_READ_DIR 7
#define SYS_CD 8
#define SYS_OPEN 9
#define SYS_CLOSE 10
#define SYS_TRUNCATE 11
/* 12 was create, folded into open as O_CREAT */
#define SYS_MKDIR 13
#define SYS_UNLINK 14
#define SYS_RMDIR 15
#define SYS_CLOCK_GETTIME 16
#define SYS_NANOSLEEP 17
#define SYS_BRK 18
#define SYS_SBRK 19
#define SYS_LSEEK 20
#define SYS_FSTAT 21
#define SYS_STAT 22
#define SYS_GETPID 23
#define SYS_READ_CHAR 24
#define SYS_RENAME 25

/* ------------------------------------------------------------------------
 * ABI types and constants
 *
 * These mirror the kernel exactly. They are deliberately not newlib's own
 * definitions: the kernel owns its layouts, and this file is the one place
 * that translates between the two.
 * ---------------------------------------------------------------------- */

/* Failures come back as the errno negated, anything outside this window is a
 * success value. */
#define BUBBLE_MAX_ERRNO 4095

/* Kernel errno values. Everything at or below 34 matches newlib, but ENOSYS
 * and ENOTEMPTY do not, which is why bubble_errno() translates rather than
 * negating straight into errno. */
#define BUBBLE_EPERM 1
#define BUBBLE_ENOENT 2
#define BUBBLE_ESRCH 3
#define BUBBLE_EIO 5
#define BUBBLE_ENOEXEC 8
#define BUBBLE_EBADF 9
#define BUBBLE_ECHILD 10
#define BUBBLE_ENOMEM 12
#define BUBBLE_EACCES 13
#define BUBBLE_EFAULT 14
#define BUBBLE_EEXIST 17
#define BUBBLE_ENOTDIR 20
#define BUBBLE_EISDIR 21
#define BUBBLE_EINVAL 22
#define BUBBLE_EMFILE 24
#define BUBBLE_ENOSPC 28
#define BUBBLE_ERANGE 34
#define BUBBLE_ENOSYS 38
#define BUBBLE_ENOTEMPTY 39

/* File type bits in bubble_stat.mode. Same values POSIX uses. */
#define BUBBLE_S_IFMT 0170000
#define BUBBLE_S_IFREG 0100000
#define BUBBLE_S_IFDIR 0040000
#define BUBBLE_S_IFCHR 0020000

#define BUBBLE_CLOCK_REALTIME 0
#define BUBBLE_CLOCK_MONOTONIC 1

/* The kernel's open flags use newlib's BSD numbering precisely so this layer
 * can pass the caller's flags through untouched. If a future newlib renumbers
 * them, this is where a translation has to go, and the symptom would be a file
 * silently truncated rather than a build failure. */

/* Limits on the argument blob handed to execute. */
#define BUBBLE_ARGV_MAX_BYTES 4096
#define BUBBLE_ARGV_MAX_COUNT 64

/* Matches FileStat in src/fs/fs.rs: 56 bytes, no padding. */
struct bubble_stat {
    uint64_t inode;
    uint32_t mode;
    uint32_t links;
    uint64_t size;
    uint32_t block_size;
    uint32_t blocks;
    int64_t accessed_time;
    int64_t modified_time;
    int64_t created_time;
};

/* Matches Timespec in src/time/mod.rs. Not newlib's struct timespec, whose
 * layout depends on its own configuration. */
struct bubble_timespec {
    int64_t tv_sec;
    int64_t tv_nsec;
};

/* ------------------------------------------------------------------------
 * Trap entry
 *
 * int 0x80 with the number in rax and arguments in rdi, rsi, rdx, r10, r8.
 * The kernel's trampoline saves and restores every register, so nothing but
 * rax is clobbered; "memory" is still needed because arguments are pointers
 * the kernel reads and writes through.
 * ---------------------------------------------------------------------- */

static inline long bubble_syscall(long number, long a0, long a1, long a2, long a3, long a4)
{
    long result;
    register long r10 __asm__("r10") = a3;
    register long r8 __asm__("r8") = a4;

    __asm__ __volatile__("int $0x80"
                         : "=a"(result)
                         : "a"(number), "D"(a0), "S"(a1), "d"(a2), "r"(r10), "r"(r8)
                         : "memory", "cc");

    return result;
}

#define bubble_syscall0(n) bubble_syscall((n), 0, 0, 0, 0, 0)
#define bubble_syscall1(n, a) bubble_syscall((n), (long)(a), 0, 0, 0, 0)
#define bubble_syscall2(n, a, b) bubble_syscall((n), (long)(a), (long)(b), 0, 0, 0)
#define bubble_syscall3(n, a, b, c) bubble_syscall((n), (long)(a), (long)(b), (long)(c), 0, 0)
#define bubble_syscall5(n, a, b, c, d, e)                                                          \
    bubble_syscall((n), (long)(a), (long)(b), (long)(c), (long)(d), (long)(e))

/* ------------------------------------------------------------------------
 * Error handling
 * ---------------------------------------------------------------------- */

/* Translates a kernel error number into newlib's.
 *
 * Uses newlib's symbolic constants rather than assuming its numeric values, so
 * this stays correct whatever <errno.h> says.
 */
static int bubble_errno(long kernel_errno)
{
    switch (kernel_errno) {
    case BUBBLE_EPERM:
        return EPERM;
    case BUBBLE_ENOENT:
        return ENOENT;
    case BUBBLE_ESRCH:
        return ESRCH;
    case BUBBLE_EIO:
        return EIO;
    case BUBBLE_ENOEXEC:
        return ENOEXEC;
    case BUBBLE_EBADF:
        return EBADF;
    case BUBBLE_ECHILD:
        return ECHILD;
    case BUBBLE_ENOMEM:
        return ENOMEM;
    case BUBBLE_EACCES:
        return EACCES;
    case BUBBLE_EFAULT:
        return EFAULT;
    case BUBBLE_EEXIST:
        return EEXIST;
    case BUBBLE_ENOTDIR:
        return ENOTDIR;
    case BUBBLE_EISDIR:
        return EISDIR;
    case BUBBLE_EINVAL:
        return EINVAL;
    case BUBBLE_EMFILE:
        return EMFILE;
    case BUBBLE_ENOSPC:
        return ENOSPC;
    case BUBBLE_ERANGE:
        return ERANGE;
    case BUBBLE_ENOSYS:
        return ENOSYS;
    case BUBBLE_ENOTEMPTY:
        return ENOTEMPTY;
    default:
        /* an error the kernel knows about and this file does not */
        return EINVAL;
    }
}

/* True when a raw return is an error rather than a value. */
static inline int bubble_failed(long result)
{
    return result < 0 && result >= -BUBBLE_MAX_ERRNO;
}

/* Turns a raw return into the POSIX -1-and-errno convention. */
static long bubble_check(long result)
{
    if (bubble_failed(result)) {
        errno = bubble_errno(-result);
        return -1;
    }

    return result;
}

/* ------------------------------------------------------------------------
 * The environment
 *
 * crt0 points environ at the envp array from the entry stack frame.
 *
 * It is deliberately not defined here. --disable-newlib-supplied-syscalls
 * does not suppress newlib's own environ.c, so defining it here as well is a
 * multiple definition at link time. newlib initialises it to an empty array,
 * which is exactly the behaviour we want for a program that starts without
 * an environment: getenv sees an empty list rather than a null pointer.
 * ---------------------------------------------------------------------- */

extern char **environ;

/* ------------------------------------------------------------------------
 * Process control
 * ---------------------------------------------------------------------- */

void _exit(int rc)
{
    bubble_syscall1(SYS_EXIT, rc);

    /* the kernel never schedules this process again */
    for (;;) {
    }
}

int _getpid(void)
{
    return (int)bubble_check(bubble_syscall0(SYS_GETPID));
}

/* Launches a program and returns immediately, unlike POSIX execve which
 * replaces the caller.
 *
 * The kernel has no way to replace the running image, so this cannot have
 * execve's semantics. It is still worth providing: newlib's system() and
 * posix_spawn() reach for it, and returning a pid is more useful than ENOSYS.
 * A caller that truly needs replacement has to notice the difference.
 *
 * The environment argument is ignored. Children inherit the parent's
 * environment in the kernel, and there is no way to hand it a different one.
 */
int _execve(char *name, char **argv, char **env)
{
    char blob[BUBBLE_ARGV_MAX_BYTES];
    size_t length = 0;
    size_t count = 0;

    (void)env;

    if (name == NULL) {
        errno = EFAULT;
        return -1;
    }

    /* pack argv into the NUL separated blob the kernel expects */
    if (argv != NULL) {
        for (count = 0; argv[count] != NULL; count++) {
            const char *argument = argv[count];
            size_t index;

            if (count == BUBBLE_ARGV_MAX_COUNT) {
                errno = E2BIG;
                return -1;
            }

            for (index = 0; argument[index] != '\0'; index++) {
                if (length + 1 >= sizeof(blob)) {
                    errno = E2BIG;
                    return -1;
                }

                blob[length++] = argument[index];
            }

            if (length >= sizeof(blob)) {
                errno = E2BIG;
                return -1;
            }

            blob[length++] = '\0';
        }
    }

    return (int)bubble_check(bubble_syscall5(SYS_EXECUTE, name, strlen(name), blob, length, count));
}

/* No fork. Creating a process means execute, which loads a fresh image rather
 * than duplicating the caller. */
int _fork(void)
{
    errno = ENOSYS;
    return -1;
}

/* No signals. */
int _kill(int pid, int sig)
{
    (void)pid;
    (void)sig;

    errno = ENOSYS;
    return -1;
}

/* No way to wait for an unspecified child.
 *
 * The kernel's wait takes a pid and nothing tracks parent/child links, so
 * "whichever child finishes first" cannot be answered. waitpid on a known pid
 * is available through _wait_for_process below.
 */
int _wait(int *status)
{
    (void)status;

    errno = ECHILD;
    return -1;
}

/* Waits for one known process and reports its exit status.
 *
 * Not part of newlib's expected set; exposed because the kernel supports it
 * and _wait cannot.
 */
int _wait_for_process(int pid, int *status)
{
    long result = bubble_check(bubble_syscall1(SYS_WAIT_FOR_PROCESS, pid));
    if (result < 0) {
        return -1;
    }

    if (status != NULL) {
        /* the kernel already masks the status to 8 bits, shift it into the
         * position the W* macros expect */
        *status = (int)((result & 0xFF) << 8);
    }

    return pid;
}

/* Hands the rest of this timeslice back to the scheduler. */
int _yield(void)
{
    bubble_syscall0(SYS_YIELD);
    return 0;
}

/* ------------------------------------------------------------------------
 * system
 *
 * newlib ships a stub that answers ENOSYS to everything, so build.mk drops
 * its object from our copy of libc.a and this takes over. Two definitions of
 * the same symbol in one archive would otherwise resolve by whichever member
 * the linker happened to extract first.
 *
 * There is no shell to hand the string to. /bin/shell.elf has no -c mode, and
 * the kernel refuses to launch it a second time anyway, so this parses the
 * command itself and executes it directly. The parsing rules are the ones the
 * shell uses, quoting and backslashes included, so a command behaves the same
 * whether it was typed or passed to system().
 *
 * What that costs: none of the things a real sh would do. No pipes, no
 * redirection, no globbing, no && or ;, no variable expansion. A caller that
 * wants any of those gets its command line treated as one program name and
 * some arguments.
 * ---------------------------------------------------------------------- */

/* Longest resolved program path a PATH search will build. */
#define BUBBLE_PATH_MAX 256

/* Where to look when the environment carries no PATH. */
#define BUBBLE_DEFAULT_PATH "/bin"

/* The exit status a shell reports for a command it could not find. */
#define BUBBLE_STATUS_NOT_FOUND 127

/* Splits a command line into arguments in place.
 *
 * Quotes group, a backslash outside quotes takes the next byte literally, and
 * an unterminated quote closes at the end of the line rather than failing the
 * whole command. Both removals only ever shorten the text, so the result is
 * written over the input as it is consumed.
 *
 * Returns the argument count, or -1 when there are more than `max_count`.
 */
static int bubble_split_command(char *command, char **argv, size_t max_count)
{
    size_t read = 0;
    size_t write = 0;
    size_t count = 0;
    char quote = '\0';
    int in_argument = 0;

    while (command[read] != '\0') {
        char byte = command[read++];

        if (quote != '\0') {
            if (byte == quote) {
                quote = '\0';
            } else {
                command[write++] = byte;
            }

            continue;
        }

        if (byte == '\'' || byte == '"') {
            quote = byte;

            /* an empty quoted string is still an argument, so open one now
             * rather than waiting for a byte that may never come */
            if (!in_argument) {
                if (count == max_count) {
                    return -1;
                }

                argv[count++] = &command[write];
                in_argument = 1;
            }

            continue;
        }

        if (byte == ' ' || byte == '\t' || byte == '\n' || byte == '\r'
            || byte == '\v' || byte == '\f') {
            if (in_argument) {
                command[write++] = '\0';
                in_argument = 0;
            }

            continue;
        }

        if (byte == '\\' && command[read] != '\0') {
            byte = command[read++];
        }

        if (!in_argument) {
            if (count == max_count) {
                return -1;
            }

            argv[count++] = &command[write];
            in_argument = 1;
        }

        command[write++] = byte;
    }

    if (in_argument) {
        command[write++] = '\0';
    }

    argv[count] = NULL;
    return (int)count;
}

/* Builds "<directory>/<program>" for one PATH entry.
 *
 * Returns 0, or -1 when the result would not fit.
 */
static int bubble_join_path(char *out, size_t out_size, const char *directory,
                            size_t directory_length, const char *program)
{
    size_t program_length = strlen(program);
    size_t length = 0;

    /* an empty PATH entry means the working directory */
    if (directory_length > 0) {
        if (directory_length + 1 + program_length + 1 > out_size) {
            return -1;
        }

        memcpy(out, directory, directory_length);
        length = directory_length;

        if (out[length - 1] != '/') {
            out[length++] = '/';
        }
    } else if (program_length + 1 > out_size) {
        return -1;
    }

    memcpy(out + length, program, program_length);
    out[length + program_length] = '\0';

    return 0;
}

int system(const char *command)
{
    /* roughly 4.5 KiB of frame here, and _execve builds another blob of the
     * same size below it. Comfortable against a 512 KiB stack, worth knowing
     * if that number ever shrinks */
    char buffer[BUBBLE_ARGV_MAX_BYTES];
    char *argv[BUBBLE_ARGV_MAX_COUNT + 1];
    char candidate[BUBBLE_PATH_MAX];

    const char *path;
    size_t length;
    int count;
    int pid = -1;
    int status = 0;

    /* POSIX wants a nonzero answer when a command processor exists, which is
     * how a caller asks whether system() is worth using at all */
    if (command == NULL) {
        return 1;
    }

    length = strlen(command);
    if (length >= sizeof(buffer)) {
        errno = E2BIG;
        return -1;
    }

    memcpy(buffer, command, length + 1);

    count = bubble_split_command(buffer, argv, BUBBLE_ARGV_MAX_COUNT);
    if (count < 0) {
        errno = E2BIG;
        return -1;
    }

    /* an empty command line succeeds without doing anything, the way sh
     * treats one */
    if (count == 0) {
        return 0;
    }

    if (strchr(argv[0], '/') != NULL) {
        /* a path, taken as given rather than searched for */
        pid = _execve(argv[0], argv, NULL);
    } else {
        path = getenv("PATH");
        if (path == NULL) {
            path = BUBBLE_DEFAULT_PATH;
        }

        while (*path != '\0') {
            const char *separator = strchr(path, ':');
            size_t entry_length =
                separator != NULL ? (size_t)(separator - path) : strlen(path);

            if (bubble_join_path(candidate, sizeof(candidate), path,
                                 entry_length, argv[0]) == 0) {
                pid = _execve(candidate, argv, NULL);
                if (pid > 0) {
                    break;
                }
            }

            if (separator == NULL) {
                break;
            }

            path = separator + 1;
        }

        /* nothing on the search path, so try the working directory */
        if (pid <= 0) {
            pid = _execve(argv[0], argv, NULL);
        }
    }

    if (pid <= 0) {
        /* not an error in system()'s terms: the child ran and failed, as far
         * as the caller is concerned. -1 is reserved for not being able to
         * make a child at all */
        return BUBBLE_STATUS_NOT_FOUND << 8;
    }

    if (_wait_for_process(pid, &status) < 0) {
        return -1;
    }

    return status;
}

/* ------------------------------------------------------------------------
 * Files
 * ---------------------------------------------------------------------- */

/* The flags pass through unchanged, see the note above where they are
 * described. There is no mode argument in the kernel call: nothing here has
 * an owner or permission bits, so it is accepted and dropped. */
int _open(char *file, int flags, int mode)
{
    (void)mode;

    if (file == NULL) {
        errno = EFAULT;
        return -1;
    }

    return (int)bubble_check(bubble_syscall3(SYS_OPEN, file, strlen(file), flags));
}

int _close(int fildes)
{
    return (int)bubble_check(bubble_syscall1(SYS_CLOSE, fildes));
}

/* Returns the number of bytes read; zero is end of file, not an error. */
int _read(int file, char *ptr, int len)
{
    if (len < 0) {
        errno = EINVAL;
        return -1;
    }

    return (int)bubble_check(bubble_syscall3(SYS_READ, file, ptr, len));
}

int _write(int file, char *ptr, int len)
{
    if (len < 0) {
        errno = EINVAL;
        return -1;
    }

    return (int)bubble_check(bubble_syscall3(SYS_WRITE, file, ptr, len));
}

int _lseek(int file, int ptr, int dir)
{
    return (int)bubble_check(bubble_syscall3(SYS_LSEEK, file, ptr, dir));
}

int _unlink(char *name)
{
    if (name == NULL) {
        errno = EFAULT;
        return -1;
    }

    return (int)bubble_check(bubble_syscall2(SYS_UNLINK, name, strlen(name)));
}

/* Moves a directory entry, which is the only way to rename on FAT.
 *
 * newlib's own rename() is _link() plus _unlink(), and FAT has no hard links,
 * so that path can never work. build.mk drops newlib's rename.o and renamer.o
 * from our copy of libc.a and this takes over, the same as system().
 *
 * Directories can be renamed where they are but not moved to another parent;
 * the kernel refuses that rather than leaving a `..` pointing at the wrong
 * place.
 */
int rename(const char *old_path, const char *new_path)
{
    if (old_path == NULL || new_path == NULL) {
        errno = EFAULT;
        return -1;
    }

    return (int)bubble_check(bubble_syscall5(SYS_RENAME, old_path, strlen(old_path), new_path,
                                             strlen(new_path), 0));
}

/* No hard links on FAT. */
int _link(char *existing, char *new)
{
    (void)existing;
    (void)new;

    errno = ENOSYS;
    return -1;
}

/* Copies kernel metadata into newlib's struct stat.
 *
 * The two layouts are independent on purpose: the kernel owns bubble_stat and
 * newlib owns struct stat, so neither can silently change the other. Fields
 * the filesystem cannot answer are zeroed rather than invented.
 */
static void bubble_copy_stat(const struct bubble_stat *from, struct stat *to)
{
    memset(to, 0, sizeof(*to));

    to->st_ino = (ino_t)from->inode;
    to->st_mode = (mode_t)from->mode;
    to->st_nlink = (nlink_t)from->links;
    to->st_size = (off_t)from->size;
    to->st_blksize = (blksize_t)from->block_size;
    to->st_blocks = (blkcnt_t)from->blocks;

    /* st_atime and friends are the portable spelling. newlib declares the
     * fields as struct timespec and defines these as macros onto the seconds
     * member, so this assigns correctly whichever way the header is built */
    to->st_atime = (time_t)from->accessed_time;
    to->st_mtime = (time_t)from->modified_time;
    to->st_ctime = (time_t)from->created_time;
}

int _fstat(int fildes, struct stat *st)
{
    struct bubble_stat info;

    if (st == NULL) {
        errno = EFAULT;
        return -1;
    }

    if (bubble_check(bubble_syscall2(SYS_FSTAT, fildes, &info)) < 0) {
        return -1;
    }

    bubble_copy_stat(&info, st);
    return 0;
}

int _stat(const char *file, struct stat *st)
{
    struct bubble_stat info;

    if (file == NULL || st == NULL) {
        errno = EFAULT;
        return -1;
    }

    if (bubble_check(bubble_syscall3(SYS_STAT, file, strlen(file), &info)) < 0) {
        return -1;
    }

    bubble_copy_stat(&info, st);
    return 0;
}

/* Whether a descriptor is a terminal.
 *
 * stdio calls this to choose line buffering over full buffering, so answering
 * wrongly makes output appear only once a buffer fills. The standard streams
 * report as character devices; everything else is a file.
 */
int _isatty(int file)
{
    struct bubble_stat info;

    if (bubble_check(bubble_syscall2(SYS_FSTAT, file, &info)) < 0) {
        return 0;
    }

    if ((info.mode & BUBBLE_S_IFMT) == BUBBLE_S_IFCHR) {
        return 1;
    }

    errno = ENOTTY;
    return 0;
}

/* Not part of newlib's expected set, exposed because the kernel has it and
 * ftruncate has nowhere else to come from. */
int _ftruncate(int file, off_t length)
{
    return (int)bubble_check(bubble_syscall2(SYS_TRUNCATE, file, length));
}

/* ------------------------------------------------------------------------
 * Directories
 * ---------------------------------------------------------------------- */

int _mkdir(const char *path, int mode)
{
    (void)mode;

    if (path == NULL) {
        errno = EFAULT;
        return -1;
    }

    return (int)bubble_check(bubble_syscall2(SYS_MKDIR, path, strlen(path)));
}

int _rmdir(const char *path)
{
    if (path == NULL) {
        errno = EFAULT;
        return -1;
    }

    return (int)bubble_check(bubble_syscall2(SYS_RMDIR, path, strlen(path)));
}

int _chdir(const char *path)
{
    if (path == NULL) {
        errno = EFAULT;
        return -1;
    }

    return (int)bubble_check(bubble_syscall2(SYS_CD, path, strlen(path)));
}

/* ------------------------------------------------------------------------
 * Memory
 * ---------------------------------------------------------------------- */

/* Grows or shrinks the heap and returns the previous break, which is where
 * the newly usable bytes start.
 *
 * The kernel answers with the new break, so the increment is subtracted back
 * off. malloc depends on getting the old one.
 */
void *_sbrk(ptrdiff_t incr)
{
    long result = bubble_syscall1(SYS_SBRK, (long)incr);

    if (bubble_failed(result)) {
        errno = bubble_errno(-result);

        /* malloc tests against (void *) -1, not null */
        return (void *)-1;
    }

    return (void *)(result - (long)incr);
}

/* ------------------------------------------------------------------------
 * Time
 * ---------------------------------------------------------------------- */

static int bubble_clock_gettime(int clock_id, struct bubble_timespec *out)
{
    return (int)bubble_check(bubble_syscall2(SYS_CLOCK_GETTIME, clock_id, out));
}

int _gettimeofday(struct timeval *ptimeval, void *ptimezone)
{
    struct bubble_timespec now;

    (void)ptimezone;

    if (ptimeval == NULL) {
        errno = EFAULT;
        return -1;
    }

    if (bubble_clock_gettime(BUBBLE_CLOCK_REALTIME, &now) < 0) {
        return -1;
    }

    ptimeval->tv_sec = (time_t)now.tv_sec;
    ptimeval->tv_usec = (suseconds_t)(now.tv_nsec / 1000);
    return 0;
}

/* Process times.
 *
 * Nothing separates user from system time, and there is no per-process
 * accounting at all, so the monotonic clock stands in for elapsed time and the
 * rest is zero. Reporting the real elapsed time is more useful to a caller
 * measuring a duration than failing outright.
 */
clock_t _times(struct tms *buf)
{
    struct bubble_timespec now;
    long long ticks;

    if (bubble_clock_gettime(BUBBLE_CLOCK_MONOTONIC, &now) < 0) {
        return (clock_t)-1;
    }

    ticks = (long long)now.tv_sec * CLOCKS_PER_SEC
            + (long long)now.tv_nsec / (1000000000L / CLOCKS_PER_SEC);

    if (buf != NULL) {
        buf->tms_utime = (clock_t)ticks;
        buf->tms_stime = 0;
        buf->tms_cutime = 0;
        buf->tms_cstime = 0;
    }

    return (clock_t)ticks;
}

/* Sleeps for a duration.
 *
 * Not part of newlib's expected set; nanosleep and sleep have nowhere else to
 * come from.
 */
int _nanosleep(const struct timespec *duration, struct timespec *remaining)
{
    struct bubble_timespec request;

    /* nothing wakes a sleep early, so there is never a remainder */
    if (remaining != NULL) {
        remaining->tv_sec = 0;
        remaining->tv_nsec = 0;
    }

    if (duration == NULL) {
        errno = EFAULT;
        return -1;
    }

    request.tv_sec = (int64_t)duration->tv_sec;
    request.tv_nsec = (int64_t)duration->tv_nsec;

    return (int)bubble_check(bubble_syscall1(SYS_NANOSLEEP, &request));
}

/* ------------------------------------------------------------------------
 * Unprefixed syscall names
 *
 * --disable-newlib-supplied-syscalls also defines MISSING_SYSCALL_NAMES when
 * newlib builds itself, and reent.h then rewrites every `_name` in the
 * reentrant wrappers to a plain `name`. So libc_a-readr.o does not call
 * _read, it calls read. That macro is a build-time -D rather than something
 * in the installed headers, so it is not in effect here, and it must not be:
 * the underscore names above are what everything else expects.
 *
 * The alias is made in the assembler rather than with
 * __attribute__((alias)), which would compare our prototypes against the
 * ones in newlib's headers and reject the mismatches. fcntl.h declares open
 * as (const char *, int, ...) against our (char *, int, int), and several
 * others differ on const in the same way. The two names are the same code
 * either way, so there is nothing for the type check to protect.
 * ---------------------------------------------------------------------- */

/* Safe to define strongly: these are precisely the names newlib left
 * undefined, so there is nothing to collide with. */
#define BUBBLE_ALIAS(name) \
	__asm__(".global " #name "\n\t.set " #name ", _" #name)

/* Not part of MISSING_SYSCALL_NAMES, so newlib may or may not carry its own
 * stub depending on the version. Weak lets a real one win instead of
 * failing the link. */
#define BUBBLE_WEAK_ALIAS(name) \
	__asm__(".weak " #name "\n\t.set " #name ", _" #name)

BUBBLE_ALIAS(close);
BUBBLE_ALIAS(execve);
BUBBLE_ALIAS(fork);
BUBBLE_ALIAS(fstat);
BUBBLE_ALIAS(getpid);
BUBBLE_ALIAS(gettimeofday);
BUBBLE_ALIAS(isatty);
BUBBLE_ALIAS(kill);
BUBBLE_ALIAS(link);
BUBBLE_ALIAS(lseek);
BUBBLE_ALIAS(mkdir);
BUBBLE_ALIAS(open);
BUBBLE_ALIAS(read);
BUBBLE_ALIAS(sbrk);
BUBBLE_ALIAS(stat);
BUBBLE_ALIAS(times);
BUBBLE_ALIAS(unlink);
BUBBLE_ALIAS(wait);
BUBBLE_ALIAS(write);

BUBBLE_WEAK_ALIAS(chdir);
BUBBLE_WEAK_ALIAS(ftruncate);
BUBBLE_WEAK_ALIAS(nanosleep);
BUBBLE_WEAK_ALIAS(rmdir);
