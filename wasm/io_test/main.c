#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>

int is_prime(long n) {
    if (n < 2) return 0;
    for (long i = 2; i * i <= n; i++) {
        if (n % i == 0) return 0;
    }
    return 1;
}

void write_test() {

    struct timespec start, end;
    printf("Test with I/O\n");

    clock_gettime(CLOCK_REALTIME, &start); 

    for (int i = 0; i < 10; i++) {
        printf("STDOUT message %d\n", i);
        
        fprintf(stderr, "STDERR message %d\n", i);
        

        usleep(50000);
    }
    FILE *file = fopen("./test.txt", "a");
    
    printf("File descriptor: %d\n", fileno(file));
    FILE *file2 = fopen("./test2.txt", "a");
    printf("File descriptor: %d\n", fileno(file2));
    FILE *file3 = fopen("./test3.txt", "a");
    printf("File descriptor: %d\n", fileno(file3));
    FILE *file4 = fopen("./test4.txt", "a");
    printf("File descriptor: %d\n", fileno(file4));
    FILE *file5 = fopen("./test5.txt", "a");
    printf("File descriptor: %d\n", fileno(file5));

    FILE *file6 = fopen("./test.txt", "a");
    if (file == NULL) {
        fprintf(stderr, "Error opening file\n");
        return;
    }

    double result = 0.0;
    long count = 0;

    for (long i = 1; i < 5000000; i++) {
        if (is_prime(i)) {
            result += sqrt(i) * log(i);  
            count++;
        }
    }
    usleep(10000000);

    fprintf(file, "Hello, World!\n");
    fclose(file);

    clock_gettime(CLOCK_REALTIME, &end);   


    double t_ns = (double)(end.tv_sec - start.tv_sec) * 1.0e3 +
              (double)(end.tv_nsec - start.tv_nsec) / 1.0e6;


    printf("Time elapsed: %.2f\n", t_ns);

    printf("Test without I/O\n");

    clock_gettime(CLOCK_REALTIME, &start); 

    
    result = 0.0;
    count = 0;

    for (long i = 1; i < 5000000; i++) {
        if (is_prime(i)) {
            result += sqrt(i) * log(i);  
            count++;
        }
    }

    usleep(50000);
    usleep(50000);
    usleep(10000000);

    clock_gettime(CLOCK_REALTIME, &end);   


    t_ns = (double)(end.tv_sec - start.tv_sec) * 1.0e3 +
              (double)(end.tv_nsec - start.tv_nsec) / 1.0e6;


    printf("Time elapsed: %.2f", t_ns);


}

void read_test() {
/*
    char buffer[100];
    ssize_t bytesRead = read(STDIN_FILENO, buffer, sizeof(buffer) - 1);
    if (bytesRead > 0) {
        buffer[bytesRead] = '\0';
        printf("Read from stdin: %s\n", buffer);
    } else {
        perror("Error reading from stdin");
    }

    bytesRead = read(STDIN_FILENO, buffer, sizeof(buffer) - 1);
    if (bytesRead > 0) {
        buffer[bytesRead] = '\0';
        printf("Read from stdin: %s\n", buffer);
    } else {
        perror("Error reading from stdin");
    }

    FILE *file = fopen("./test.txt", "r");
    if (!file) {
        perror("Error opening file");
        return;
    }

    while (fgets(buffer, sizeof(buffer), file)) {
        printf("Read from file: %s", buffer);
    }

    fclose(file);

    */
}

int main() {
    write_test();
    read_test();

    return 0;
}
