/*
 * iops - the sequential-write microbenchmark behind the WASI hostcall
 * batching experiment (Peridot paper, Section 4.3, Figure 7).
 *
 * This is write_1g/main.c with its two hardcoded constants (DOT_NUM writes,
 * 30 repetitions) lifted into arguments, so a reviewer can run it at a scale
 * that fits the time budget of an artifact evaluation instead of the hours
 * the paper-scale configuration takes.
 *
 * Usage: iops <bytes_per_write> [writes_per_run] [runs]
 * Prints one CSV line per run plus a summary line, both on stdout:
 *   run,<i>,<bytes>,<writes>,<seconds>,<kiops>
 *   summary,<bytes>,<writes>,<runs>,<mean_kiops>,<stddev_kiops>
 */
#include <fcntl.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define TEST_FILE "iops.file"

static double now_seconds(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec / 1.0e9;
}

/* Same payload shape as write_1g: a "start"..dots.."end\n" record, so the
   bytes on the wire are identical to the original benchmark. */
static char *create_buffer(size_t size) {
    const char *prefix = "start";
    const char *suffix = "end";
    size_t plen = strlen(prefix), slen = strlen(suffix);

    char *buf = malloc(size);
    if (!buf) return NULL;

    if (size < plen + slen + 1) {
        memset(buf, '.', size);
        return buf;
    }
    memcpy(buf, prefix, plen);
    memset(buf + plen, '.', size - plen - slen - 1);
    memcpy(buf + size - slen - 1, suffix, slen);
    buf[size - 1] = '\n';
    return buf;
}

static double one_run(size_t bytes, uint64_t writes, const char *buf) {
    int fd = open(TEST_FILE, O_WRONLY | O_CREAT | O_APPEND, 0666);
    if (fd < 0) {
        perror("open");
        exit(1);
    }
    const double start = now_seconds();
    for (uint64_t i = 0; i < writes; i++) {
        if (write(fd, buf, bytes) < 0) {
            perror("write");
            exit(1);
        }
    }
    close(fd);
    return now_seconds() - start;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        printf("Usage: %s <bytes_per_write> [writes_per_run] [runs]\n", argv[0]);
        return 1;
    }

    size_t bytes = (size_t)strtoull(argv[1], NULL, 10);
    uint64_t writes = argc > 2 ? strtoull(argv[2], NULL, 10) : 1048576;
    int runs = argc > 3 ? atoi(argv[3]) : 10;

    if (bytes == 0 || writes == 0 || runs <= 0) {
        printf("bytes_per_write, writes_per_run and runs must all be > 0\n");
        return 1;
    }

    char *buf = create_buffer(bytes);
    if (!buf) {
        perror("malloc");
        return 1;
    }

    double *kiops = malloc(sizeof(double) * (size_t)runs);
    double sum = 0.0;

    for (int i = 0; i < runs; i++) {
        remove(TEST_FILE);
        double seconds = one_run(bytes, writes, buf);
        kiops[i] = seconds > 0 ? (double)writes / seconds / 1000.0 : 0.0;
        sum += kiops[i];
        printf("run,%d,%zu,%llu,%.6f,%.3f\n",
               i, bytes, (unsigned long long)writes, seconds, kiops[i]);
        fflush(stdout);
    }
    remove(TEST_FILE);

    double mean = sum / runs;
    double var = 0.0;
    for (int i = 0; i < runs; i++) var += pow(kiops[i] - mean, 2);
    double stddev = sqrt(var / runs);

    printf("summary,%zu,%llu,%d,%.3f,%.3f\n",
           bytes, (unsigned long long)writes, runs, mean, stddev);
    fflush(stdout);

    free(kiops);
    free(buf);
    return 0;
}
