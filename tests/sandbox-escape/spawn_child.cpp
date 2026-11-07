// Tries to start child processes. Prints BLOCKED or LEAK.
#include <windows.h>
#include <cstdio>
#include <cstdlib>
int main() {
    STARTUPINFOA si{}; si.cb = sizeof si; PROCESS_INFORMATION pi{};
    char cmd[] = "C:\Windows\System32\cmd.exe /c exit 0";
    bool leak = CreateProcessA(nullptr, cmd, nullptr, nullptr, FALSE, CREATE_NO_WINDOW, nullptr, nullptr, &si, &pi);
    if (leak) { WaitForSingleObject(pi.hProcess, 5000); }
    int rc = system("C:\Windows\System32\cmd.exe /c exit 7");
    if (rc == 7) leak = true;
    puts(leak ? "LEAK" : "BLOCKED");
}
