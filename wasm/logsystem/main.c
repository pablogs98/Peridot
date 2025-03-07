#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

double randfrom(double min, double max)
{
    double range = (max - min);
    double div = RAND_MAX / range;
    return min + (rand() / div);
}

double get_current_time() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec / 1000000000.0;
}

int main(int argc, char *argv[]) {

    srand(time(NULL));

    if (argc != 3) {
        printf("Use: %s <file> <execution time in seconds>\n", argv[0]);
        return 1;
    }
    char *FILE_NAME = argv[1];
    double execution_time = atof(argv[2]);

    int buffer_sizes[] = {1024, 4096, 16384, 65536, 262144};

    FILE *file = fopen(FILE_NAME, "a");

    if (!file) {
        printf("Error opening file %s\n", FILE_NAME);
        return 1;
    }

    double start_time = get_current_time();

    while (1) {
        if (get_current_time() - start_time > execution_time) {
            break;
        }

        // pick a random buffer size
        int buffer_size = buffer_sizes[rand() % 5];
        char *buffer = malloc(buffer_size);

        for (int i = 0; i < buffer_size; i++) {
            buffer[i] = rand() % 256;
        }

        fwrite(buffer, 1, buffer_size, file);

        printf("Wrote %d bytes\n", buffer_size);

        //sleep for a random time between 0.2 and 0.5 seconds
        usleep(randfrom(200000, 500000));
        free(buffer);
    }

    fclose(file);
    printf("Execution time: %f\n", execution_time);
    return 0;
}
