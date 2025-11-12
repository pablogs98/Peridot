#include <assert.h>
#include <fcntl.h>
#include <limits.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

int buffer_size = 1;
uint64_t rand_lim_page = 0;

#define DOT_NUM 0x400000

#define FILE_POS 1 //0: memory 1: ssd
#define WITH_SUM 0

#define SSD_O_DIRECT_SWITCH 0 // 0: off 1:on

#define TEST_FILE "test.file"
#if SSD_O_DIRECT_SWITCH
    #define FLAGS O_DIRECT | OCREAT
#else
    #define FLAGS O_CREAT
#endif

char* create_buffer(size_t buffer_size) {
    const char* prefix = "start";
    const char* suffix = "end";
    size_t prefix_len = strlen(prefix); // 5
    size_t suffix_len = strlen(suffix); // 3

    // Must have room for prefix + suffix + at least 0 dots + '\n'
    if (buffer_size < prefix_len + suffix_len + 1) {
        return NULL; // Not enough space
    }

    char* buffer = malloc(buffer_size);
    if (!buffer) return NULL;

    size_t dots_len = buffer_size - prefix_len - suffix_len - 1;

    // Fill buffer
    memcpy(buffer, prefix, prefix_len);
    memset(buffer + prefix_len, '.', dots_len);
    memcpy(buffer + prefix_len + dots_len, suffix, suffix_len);
    buffer[buffer_size - 1] = '\n';

    return buffer;
}

double get_current_time() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec / 1.0e9;
}

double syscall_read(){
    int const fd = open(TEST_FILE, O_WRONLY | O_CREAT | O_APPEND, 0666);
    printf("File descriptor: %d\n", fd);

    if (fd < 0) {
        perror("Error opening file");
        exit(1);
    }
#define USE_PAGES 16384
#define PAGE 131072

    char* buf = create_buffer(buffer_size);

    int i = 0;
    const double start_time = get_current_time();
    for(i=0; i < DOT_NUM; i++){
        write(fd, buf, buffer_size);
    }
    close(fd);
    const double end_time = get_current_time();
    free(buf);
    return end_time - start_time;
}


int main(int argc, char** argv){
    char cwd[PATH_MAX];
    if (getcwd(cwd, sizeof(cwd)) != NULL) {
        printf("Current working dir: %s\n", cwd);
    } else {
        perror("getcwd() error");
        return 1;
    }
    buffer_size = atoi(argv[1]);
    assert(buffer_size > 0);

    printf("File POS: %s\n", TEST_FILE);
    printf("Buffer SIZE: %d\n", buffer_size);

    double sum_iops = 0;
    double iops[30];

    for (int i = 0; i < 30; i++) {
        remove(TEST_FILE);
        double const time = syscall_read();
        iops[i] = DOT_NUM / time / 1000;
        sum_iops += iops[i];
        printf("Average IO time:\t%.3f us\n" , time * 1000.0 * 1000.0);
    }

    double mean = sum_iops / 30;
    double values = 0;
    for (int i = 0; i < 30; i++) {
        values += pow(iops[i] - mean, 2);
    }
    double variance = values / 30;
    double standardDeviation = sqrt(variance);

    printf("Total IOPS:\t\t\t%.2lf K\n", mean);
    printf("Standard Deviation:\t%.2lf K\n", standardDeviation);
    return 0;
}
