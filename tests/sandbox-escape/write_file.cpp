// Tries to create the file given in argv[1]. Prints BLOCKED or LEAK.
#include <cstdio>
int main(int argc, char** argv) {
    FILE* f = fopen(argv[1], "wb");
    if (!f) { puts("BLOCKED"); return 0; }
    fputs("pwned", f); fclose(f); puts("LEAK");
}
