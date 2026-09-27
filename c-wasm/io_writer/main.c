/*
 * io_writer - the I/O-intensive guest used by the dynamic I/O provisioning
 * experiment (Peridot paper, Section 4.1, Figure 5).
 *
 * It writes a fixed total volume in chunks whose size follows a normal
 * distribution clamped to [1, 10] MiB, and prints one progress line per
 * second giving the bytes written so far. Those lines are what the experiment
 * script turns into a per-module bandwidth time series.
 *
 * Usage: io_writer <file> <total_MiB> [mean_MiB] [stddev_MiB]
 */
#include <fcntl.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define MIN_CHUNK (1u << 20)       /* 1 MiB  */
#define MAX_CHUNK (10u << 20)      /* 10 MiB */

static double now_seconds(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec / 1.0e9;
}

/* Box-Muller. rand() is seeded per module so the four collocated instances do
   not all draw the same chunk sequence. */
static double gaussian(double mean, double stddev) {
    double u1 = (rand() + 1.0) / ((double)RAND_MAX + 2.0);
    double u2 = (rand() + 1.0) / ((double)RAND_MAX + 2.0);
    return mean + stddev * sqrt(-2.0 * log(u1)) * cos(2.0 * M_PI * u2);
}

int main(int argc, char **argv) {
    if (argc < 3) {
        printf("Usage: %s <file> <total_MiB> [mean_MiB] [stddev_MiB]\n", argv[0]);
        return 1;
    }

    const char *path = argv[1];
    uint64_t total = (uint64_t)strtoull(argv[2], NULL, 10) << 20;
    double mean = (argc > 3 ? atof(argv[3]) : 5.5) * (1u << 20);
    double stddev = (argc > 4 ? atof(argv[4]) : 1.5) * (1u << 20);

    srand((unsigned)(now_seconds() * 1000.0) ^ (unsigned)(uintptr_t)path);

    char *buf = malloc(MAX_CHUNK);
    if (!buf) {
        perror("malloc");
        return 1;
    }
    memset(buf, 'p', MAX_CHUNK);

    int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) {
        perror("open");
        free(buf);
        return 1;
    }

    const double start = now_seconds();
    double next_report = start + 1.0;
    double last_report_t = 0.0;
    uint64_t written = 0;

    /* The header lets the parser tell this module's output apart when several
       instances share a log directory. */
    printf("io_writer start file=%s total_bytes=%llu\n",
           path, (unsigned long long)total);
    fflush(stdout);

    while (written < total) {
        double d = gaussian(mean, stddev);
        if (d < MIN_CHUNK) d = MIN_CHUNK;
        if (d > MAX_CHUNK) d = MAX_CHUNK;
        size_t chunk = (size_t)d;
        if (chunk > total - written) chunk = (size_t)(total - written);

        ssize_t n = write(fd, buf, chunk);
        if (n < 0) {
            perror("write");
            break;
        }
        written += (uint64_t)n;

        double t = now_seconds();
        if (t >= next_report) {
            last_report_t = t - start;
            printf("progress t=%.3f written=%llu\n",
                   last_report_t, (unsigned long long)written);
            fflush(stdout);
            next_report = t >= next_report + 1.0 ? t + 1.0 : next_report + 1.0;
        }
    }

    double elapsed = now_seconds() - start;
    /* Only emit a closing sample if it is far enough from the last periodic
       one to carry a meaningful interval. Emitting it unconditionally
       produces two points a few microseconds apart, and differentiating
       those gives a meaningless bandwidth spike. */
    if (elapsed > last_report_t + 0.2) {
        printf("progress t=%.3f written=%llu\n", elapsed, (unsigned long long)written);
    }
    printf("io_writer done elapsed=%.3f written=%llu mean_Bps=%.0f\n",
           elapsed, (unsigned long long)written,
           elapsed > 0 ? written / elapsed : 0.0);
    fflush(stdout);

    close(fd);
    free(buf);
    return 0;
}
