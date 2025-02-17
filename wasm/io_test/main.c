#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

void write_test() {
    for (int i = 0; i < 10; i++) {
        printf("STDOUT message %d\n", i);
        // print to stderr
        fprintf(stderr, "STDERR message %d\n", i);
        // print to stdout

        usleep(50000);
    }
    FILE *file = fopen("./test.txt", "a");
    //print the fd for this file
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

    // Escribir al final del archivo sin sobrescribir
    fprintf(file, "Hello, World!\n");
    fclose(file);
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

    FILE *file = fopen("./test.txt", "a");
    fprintf(file, "Hello, World!\n");
    FILE *file2 = fopen("./test.txt2", "a");
    fprintf(file2, "Hello, World!\n");
    FILE *file3 = fopen("./test.txt3", "a");
    fprintf(file3, "Hello, World!\n");
    FILE *file4 = fopen("./test.txt4", "a");
    fprintf(file4, "Hello, World!\n");
    FILE *file5 = fopen("./test.txt5", "a");
    fprintf(file5, "Hello, World!\n");

    fclose(file);
    fclose(file2);
    fclose(file3);
    fclose(file4);
    fclose(file5);
}

int main() {
    write_test();
    read_test();

    return 0;
}
