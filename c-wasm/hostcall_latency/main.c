/*
 * hostcall_latency - the interposition-overhead microbenchmark
 * (Peridot paper, Section 3.2, Figure 3).
 *
 * Times the four WASI I/O hostcalls a context interposes on -- fd_write,
 * fd_pwrite, fd_read, fd_pread -- at 1 KiB per operation, using the guest's
 * own monotonic clock, exactly as the paper describes. Run it once with an
 * empty context chain and once with a context installed; the difference is
 * the interposition cost.
 *
 * Emits one CSV line per hostcall on stdout:
 *   op,<name>,<bytes>,<iterations>,<mean_us>,<p50_us>,<p99_us>,<stddev_us>
 *
 * Usage: hostcall_latency [bytes_per_op] [iterations]
 */
#include <fcntl.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define TEST_FILE "latency.file"

static double now_us(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1.0e6 + ts.tv_nsec / 1.0e3;
}

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}

/* Reports mean, median, p99 and standard deviation over `n` samples.
   Percentiles matter here because the interposition cost is small enough that
   a few scheduler hiccups would dominate a mean-only comparison. */
static void report(const char *op, size_t bytes, size_t n, double *s) {
    qsort(s, n, sizeof(double), cmp_double);

    double sum = 0.0;
    for (size_t i = 0; i < n; i++) sum += s[i];
    double mean = sum / n;

    double var = 0.0;
    for (size_t i = 0; i < n; i++) var += (s[i] - mean) * (s[i] - mean);

    printf("op,%s,%zu,%zu,%.4f,%.4f,%.4f,%.4f\n",
           op, bytes, n, mean, s[n / 2], s[(size_t)(n * 0.99)],
           sqrt(var / n));
    fflush(stdout);
}

int main(int argc, char **argv) {
    size_t bytes = argc > 1 ? (size_t)strtoull(argv[1], NULL, 10) : 1024;
    size_t iters = argc > 2 ? (size_t)strtoull(argv[2], NULL, 10) : 20000;

    if (bytes == 0 || iters == 0) {
        printf("bytes_per_op and iterations must both be > 0\n");
        return 1;
    }

    char *buf = malloc(bytes);
    char *rbuf = malloc(bytes);
    double *samples = malloc(sizeof(double) * iters);
    if (!buf || !rbuf || !samples) {
        perror("malloc");
        return 1;
    }
    memset(buf, 'p', bytes);

    remove(TEST_FILE);
    int fd = open(TEST_FILE, O_RDWR | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) {
        perror("open");
        return 1;
    }

    /* Pre-fill so the read paths always have `bytes` to return and are not
       measuring short reads at EOF. */
    for (size_t i = 0; i < iters; i++) {
        if (write(fd, buf, bytes) < 0) { perror("prefill"); return 1; }
    }
    lseek(fd, 0, SEEK_SET);

    /* Warm-up: the first calls pay for lazily-built context state and a cold
       file cache, which would otherwise land entirely in the Peridot arm. */
    for (size_t i = 0; i < 1000 && i < iters; i++) {
        write(fd, buf, bytes);
        pread(fd, rbuf, bytes, 0);
    }

    lseek(fd, 0, SEEK_SET);
    for (size_t i = 0; i < iters; i++) {
        double t0 = now_us();
        write(fd, buf, bytes);
        samples[i] = now_us() - t0;
    }
    report("fd_write", bytes, iters, samples);

    for (size_t i = 0; i < iters; i++) {
        double t0 = now_us();
        pwrite(fd, buf, bytes, (off_t)(i * bytes));
        samples[i] = now_us() - t0;
    }
    report("fd_pwrite", bytes, iters, samples);

    lseek(fd, 0, SEEK_SET);
    for (size_t i = 0; i < iters; i++) {
        double t0 = now_us();
        read(fd, rbuf, bytes);
        samples[i] = now_us() - t0;
    }
    report("fd_read", bytes, iters, samples);

    for (size_t i = 0; i < iters; i++) {
        double t0 = now_us();
        pread(fd, rbuf, bytes, (off_t)(i * bytes));
        samples[i] = now_us() - t0;
    }
    report("fd_pread", bytes, iters, samples);

    close(fd);
    remove(TEST_FILE);
    free(buf);
    free(rbuf);
    free(samples);
    return 0;
}
