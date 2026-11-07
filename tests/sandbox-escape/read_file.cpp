// Tries to read the file given in argv[1]. Prints BLOCKED or LEAK.
#include <cstdio>
int main(int argc, char** argv) {
    FILE* f = fopen(argv[1], "rb");
    if (!f) { puts("BLOCKED"); return 0; }
    char buf[16]; size_t n = fread(buf, 1, sizeof buf, f); fclose(f);
    printf(n ? "LEAK\n" : "BLOCKED\n");
}
