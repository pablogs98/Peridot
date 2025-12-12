#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#include <stdint.h>

// Compute pi using the Bailey–Borwein–Plouffe (BBP) formula
double compute_pi_bbp(uint64_t iterations) {
    double pi = 0.0;

    for (uint64_t k = 0; k < iterations; k++) {
        double term =
            (1.0 / pow(16.0, k)) *
            (4.0 / (8*k + 1) -
             2.0 / (8*k + 4) -
             1.0 / (8*k + 5) -
             1.0 / (8*k + 6));

        pi += term;
    }

    return pi;
}

int main(int argc, char *argv[]) {
    uint64_t iterations = 100000000; // 100 million (heavy workload)

    if (argc > 1) {
        iterations = strtoull(argv[0], NULL, 10);
    }

    printf("Running CPU-intensive computation...\n");
    double pi = compute_pi_bbp(iterations);

    printf("Approximation of PI = %.15f\n", pi);
    printf("Iterations used: %llu\n", (unsigned long long)iterations);

    return 0;
}