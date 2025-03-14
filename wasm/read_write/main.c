#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <fcntl.h>

// Generate a random double within a given range
double randfrom(double min, double max) {
    double range = (max - min);
    double div = RAND_MAX / range;
    return min + (rand() / div);
}

// Get current time in nanoseconds
double get_current_time() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec / 1.0e9;
}

int main(int argc, char *argv[]) {
    srand(time(NULL));

    if (argc != 3) {
        printf("Use: %s <file> <execution time in seconds>\n", argv[0]);
        return 1;
    }

    char *FILE_NAME = argv[1];
    double execution_time = atof(argv[2]);
    int buffer_size = 1024;
    char *buffer = malloc(buffer_size);
    for (int i = 0; i < buffer_size; i++) {
        buffer[i] = rand() % 256;
    }

    double start_time = get_current_time();

    while (1) {
        if (get_current_time() - start_time > execution_time) {
            break;
        }

        int file = open(FILE_NAME, O_RDWR | O_CREAT, 0644);
        if (file == -1) {
            printf("Error opening file %s\n", FILE_NAME);
            return 1;
        }



        // Write buffer to the file
        write(file, buffer, buffer_size);
        
        pwrite(file, buffer, buffer_size, 0);

        // Move file pointer to the beginning
        lseek(file, 0, SEEK_SET);

        char *read_buffer = malloc(buffer_size);

        // Read from file
        read(file, read_buffer, buffer_size);

        // Read from a specific position
        pread(file, read_buffer, buffer_size, 0);

        // Force data sync to disk
        fdatasync(file);

        // Sleep for a random time between 0.2 and 0.5 seconds
        usleep(200000 + rand() % 580000);

        free(buffer);
        free(read_buffer);
        close(file);
    }
    return 0;
}
