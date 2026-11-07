// Tries to read the clipboard. Prints BLOCKED or LEAK.
#include <windows.h>
#include <cstdio>
int main() {
    if (!OpenClipboard(nullptr)) { puts("BLOCKED"); return 0; }
    HANDLE h = GetClipboardData(CF_TEXT);
    CloseClipboard();
    puts(h ? "LEAK" : "BLOCKED");
}
