// Pause the actual Darwin pipe() return before Rust sets FD_CLOEXEC.
#include <fcntl.h>
#include <limits.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>

static atomic_int used = 0;

static int delayed_pipe(int *fds) {
    int result = pipe(fds);
    const char *root = getenv("DEADPAN_PIPE_PROBE_ROOT");
    if (result != 0 || root == NULL) return result;
    char arm[PATH_MAX], ready[PATH_MAX], release[PATH_MAX];
    if (snprintf(arm, sizeof(arm), "%s/arm", root) >= (int)sizeof(arm) ||
        snprintf(ready, sizeof(ready), "%s/ready", root) >= (int)sizeof(ready) ||
        snprintf(release, sizeof(release), "%s/release", root) >= (int)sizeof(release)) {
        _exit(91);
    }
    if (access(arm, F_OK) != 0 || atomic_exchange(&used, 1) != 0) return result;
    int marker = open(ready, O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (marker < 0 || close(marker) != 0) _exit(92);
    struct timespec start, now;
    if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) _exit(93);
    while (access(release, F_OK) != 0) {
        if (clock_gettime(CLOCK_MONOTONIC, &now) != 0 || now.tv_sec - start.tv_sec >= 5) {
            _exit(94);
        }
        usleep(1000);
    }
    return result;
}

__attribute__((used)) static struct {
    const void *replacement;
    const void *original;
} interpose __attribute__((section("__DATA,__interpose"))) = {
    (const void *)delayed_pipe, (const void *)pipe
};
