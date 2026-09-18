#define _GNU_SOURCE
#include <errno.h>
#include <inttypes.h>
#include <linux/perf_event.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/resource.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

static long perf_event_open(struct perf_event_attr *attr, pid_t pid, int cpu,
                            int group_fd, unsigned long flags) {
    return syscall(__NR_perf_event_open, attr, pid, cpu, group_fd, flags);
}

static int write_metrics(const char *path, const char *status, int status_errno,
                         uint64_t branch_misses, uint64_t max_rss_bytes) {
    FILE *f = fopen(path, "w");
    if (f == NULL) {
        fprintf(stderr, "cannot open metrics file %s: %s\n", path, strerror(errno));
        return -1;
    }
    fprintf(f, "schema=elasticxxx-be13-process-metrics/v1\n");
    fprintf(f, "branch_counter_status=%s\n", status);
    fprintf(f, "branch_counter_errno=%d\n", status_errno);
    if (strcmp(status, "measured") == 0) {
        fprintf(f, "branch_misses=%" PRIu64 "\n", branch_misses);
    } else {
        fprintf(f, "branch_misses=unmeasured\n");
    }
    fprintf(f, "max_rss_bytes=%" PRIu64 "\n", max_rss_bytes);
    fprintf(f, "branch_scope=whole_process_user_space_including_loader_setup_warmup_timed_region_and_output\n");
    fprintf(f, "memory_scope=whole_process_peak_resident_set_via_wait4_ru_maxrss\n");
    if (fclose(f) != 0) {
        fprintf(stderr, "cannot close metrics file %s: %s\n", path, strerror(errno));
        return -1;
    }
    return 0;
}

static int child_exit_code(int status) {
    if (WIFEXITED(status)) {
        return WEXITSTATUS(status);
    }
    if (WIFSIGNALED(status)) {
        return 128 + WTERMSIG(status);
    }
    return 125;
}

int main(int argc, char **argv) {
    if (argc < 4 || strcmp(argv[2], "--") != 0) {
        fprintf(stderr, "usage: %s METRICS_FILE -- COMMAND [ARG ...]\n", argv[0]);
        return 2;
    }

    const char *metrics_path = argv[1];
    pid_t child = fork();
    if (child < 0) {
        perror("fork");
        return 3;
    }
    if (child == 0) {
        if (raise(SIGSTOP) != 0) {
            _exit(126);
        }
        execvp(argv[3], &argv[3]);
        fprintf(stderr, "execvp %s failed: %s\n", argv[3], strerror(errno));
        _exit(127);
    }

    int stopped_status = 0;
    if (waitpid(child, &stopped_status, WUNTRACED) != child || !WIFSTOPPED(stopped_status)) {
        fprintf(stderr, "child did not enter synchronization stop\n");
        kill(child, SIGKILL);
        (void)waitpid(child, NULL, 0);
        return 4;
    }

    struct perf_event_attr pe;
    memset(&pe, 0, sizeof(pe));
    pe.type = PERF_TYPE_HARDWARE;
    pe.size = sizeof(pe);
    pe.config = PERF_COUNT_HW_BRANCH_MISSES;
    pe.disabled = 1;
    pe.exclude_kernel = 1;
    pe.exclude_hv = 1;

    int perf_errno = 0;
    int perf_fd = (int)perf_event_open(&pe, child, -1, -1, PERF_FLAG_FD_CLOEXEC);
    if (perf_fd < 0) {
        perf_errno = errno;
    } else {
        if (ioctl(perf_fd, PERF_EVENT_IOC_RESET, 0) != 0 ||
            ioctl(perf_fd, PERF_EVENT_IOC_ENABLE, 0) != 0) {
            perf_errno = errno;
            close(perf_fd);
            perf_fd = -1;
        }
    }

    if (kill(child, SIGCONT) != 0) {
        perror("SIGCONT");
        if (perf_fd >= 0) {
            close(perf_fd);
        }
        kill(child, SIGKILL);
        (void)waitpid(child, NULL, 0);
        return 5;
    }

    struct rusage usage;
    memset(&usage, 0, sizeof(usage));
    int child_status = 0;
    if (wait4(child, &child_status, 0, &usage) != child) {
        perror("wait4");
        if (perf_fd >= 0) {
            close(perf_fd);
        }
        return 6;
    }

    uint64_t branch_misses = 0;
    const char *counter_status = "unavailable";
    if (perf_fd >= 0) {
        if (ioctl(perf_fd, PERF_EVENT_IOC_DISABLE, 0) != 0) {
            perf_errno = errno;
        } else {
            ssize_t n = read(perf_fd, &branch_misses, sizeof(branch_misses));
            if (n == (ssize_t)sizeof(branch_misses)) {
                counter_status = "measured";
                perf_errno = 0;
            } else {
                perf_errno = n < 0 ? errno : EIO;
            }
        }
        close(perf_fd);
    }

    uint64_t max_rss_bytes = 0;
#if defined(__linux__)
    if (usage.ru_maxrss > 0 && (uint64_t)usage.ru_maxrss <= UINT64_MAX / 1024u) {
        max_rss_bytes = (uint64_t)usage.ru_maxrss * 1024u;
    }
#else
    if (usage.ru_maxrss > 0) {
        max_rss_bytes = (uint64_t)usage.ru_maxrss;
    }
#endif

    if (write_metrics(metrics_path, counter_status, perf_errno, branch_misses,
                      max_rss_bytes) != 0) {
        return 7;
    }

    return child_exit_code(child_status);
}
