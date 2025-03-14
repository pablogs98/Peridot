#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "include/sqlite3.h"

char *read_sql_file(const char *filename) {
    FILE *file = fopen(filename, "r");
    if (!file) {
        printf("Error opening file %s\n", filename);
        return nullptr;
    }

    fseek(file, 0, SEEK_END);
    long length = ftell(file);
    rewind(file);

    char *sql = malloc(length + 1);
    if (!sql) {
        printf("Error allocating memory for SQL query\n");
        fclose(file);
        return NULL;
    }

    fread(sql, 1, length, file);
    sql[length] = '\0';
    fclose(file);
    return sql;
}

double get_current_time() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec / 1000000000.0;
}

int main() {
    sqlite3 *db = NULL;
    sqlite3_stmt *stmt = NULL;

    printf("Opening DB...\n");

    int rc = sqlite3_open("/home/malvarez/Documents/WASM/peridot/wasm/sqlite/TPC-H-30.db", &db);
    if (rc != SQLITE_OK) {
        printf("Error: %s\n", sqlite3_errmsg(db));
        return 1;
    }

    //sqlite3_exec(db, "PRAGMA cache_size = 0;", 0, 0, 0);
    //sqlite3_exec(db, "PRAGMA temp_store = FILE;", 0, 0, 0);
    //sqlite3_exec(db, "PRAGMA synchronous = FULL;", 0, 0, 0);

    for (int i = 1; i <= 3; i++) {
        char filename[200];
        sprintf(filename, "/home/malvarez/Documents/WASM/peridot/wasm/sqlite/queries/%d.sql", i);

        char *sql = read_sql_file(filename);
        if (!sql) {
            continue;
        }

        char output_filename[200];
        sprintf(output_filename, "/home/malvarez/Documents/WASM/peridot/wasm/sqlite/tmp/%d.txt", i);

        FILE *output_file = fopen(output_filename, "w");
        if (!output_file) {
            printf("Error opening output file: %s\n", output_filename);
            free(sql);
            continue;
        }

        double start_time = get_current_time();

        rc = sqlite3_prepare_v2(db, sql, -1, &stmt, NULL);
        if (rc != SQLITE_OK) {
            fprintf(output_file, "Error preparing %s: %s\n", filename, sqlite3_errmsg(db));
            fclose(output_file);
            free(sql);
            continue;
        }

        while (sqlite3_step(stmt) == SQLITE_ROW) {
            int cols = sqlite3_column_count(stmt);
            for (int j = 0; j < cols; j++) {
                const char *col_value = (const char *)sqlite3_column_text(stmt, j);
                //fprintf(output_file, "%s | ", col_value ? col_value : "NULL");
            }
            //fprintf(output_file, "\n");
        }

        double end_time = get_current_time();
        printf("Execution time for query %d: %f s\n", i, end_time - start_time);

        sqlite3_finalize(stmt);
        fclose(output_file);
        free(sql);
    }

    sqlite3_close(db);
    return 0;
}
