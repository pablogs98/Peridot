#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <fcntl.h>
#include <time.h>

// Get current time in seconds (double)
double get_current_time() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec / 1.0e9;
}

int main(int argc, char *argv[]) {
    if (argc != 4) {
        printf("Use: %s <file> <bytes_per_write> <seconds_between_writes>\n", argv[0]);
        return 1;
    }

    const char *file_name = argv[1];
    size_t bytes_per_write = atoi(argv[2]);
    double interval = atof(argv[3]);

    // Allocate and fill buffer
    char *buffer = malloc(bytes_per_write);
    if (!buffer) {
        perror("malloc");
        return 1;
    }
    for (size_t i = 0; i < bytes_per_write; i++) {
        buffer[i] = rand() % 256;
    }

    printf("Writing %zu bytes to '%s' every %.2f seconds...\n",
           bytes_per_write, file_name, interval);

    // Open file once, keep open
    int fd = open(file_name, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd == -1) {
        perror("open");
        free(buffer);
        return 1;
    }

    while (1) {
        ssize_t written = write(fd, buffer, bytes_per_write);
        if (written < 0) {
            perror("write");
            break;
        }

        // Ensure data is flushed to disk (optional)
        fsync(fd);

        printf("Wrote %zd bytes at time %.2f s\n",
               written, get_current_time());

        // Wait before next write
        sleep((unsigned int)interval);
    }

    close(fd);
    free(buffer);
    return 0;
}