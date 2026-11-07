// Diagnostic: print the environment and try to write into TEMP / LOCALAPPDATA.
#include <cstdio>
#include <cstdlib>
#include <string>
extern char** environ;
static void probe(const char* var) {
    const char* d = getenv(var); if (!d) { printf("%s unset\n", var); return; }
    std::string p = std::string(d) + "/oij_probe.txt";
    FILE* f = fopen(p.c_str(), "wb");
    printf("write %s -> %s\n", p.c_str(), f ? "LEAK" : "BLOCKED");
    if (f) { fclose(f); remove(p.c_str()); }
}
int main() { for (char** e = environ; *e; ++e) puts(*e); probe("TEMP"); probe("LOCALAPPDATA"); probe("APPDATA"); probe("USERPROFILE"); }
