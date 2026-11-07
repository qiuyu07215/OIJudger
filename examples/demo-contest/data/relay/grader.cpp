// Starts the contestant's encoder.exe and decoder.exe and relays data.
#include <windows.h>
#include <cstdio>
static int run(const char* exe, const char* in, const char* out) {
    SECURITY_ATTRIBUTES sa{sizeof sa, nullptr, TRUE};
    HANDLE hi = CreateFileA(in, GENERIC_READ, FILE_SHARE_READ, &sa, OPEN_EXISTING, 0, nullptr);
    HANDLE ho = CreateFileA(out, GENERIC_WRITE, 0, &sa, CREATE_ALWAYS, 0, nullptr);
    if (hi == INVALID_HANDLE_VALUE || ho == INVALID_HANDLE_VALUE) return -1;
    STARTUPINFOA si{}; si.cb = sizeof si; si.dwFlags = STARTF_USESTDHANDLES;
    si.hStdInput = hi; si.hStdOutput = ho; si.hStdError = GetStdHandle(STD_ERROR_HANDLE);
    PROCESS_INFORMATION pi{};
    char cmd[64]; snprintf(cmd, sizeof cmd, "%s", exe);
    if (!CreateProcessA(exe, cmd, nullptr, nullptr, TRUE, 0, nullptr, nullptr, &si, &pi)) {
        CloseHandle(hi); CloseHandle(ho); return -2;
    }
    WaitForSingleObject(pi.hProcess, INFINITE);
    DWORD code = 1; GetExitCodeProcess(pi.hProcess, &code);
    CloseHandle(pi.hProcess); CloseHandle(pi.hThread); CloseHandle(hi); CloseHandle(ho);
    return (int)code;
}
int main() {
    long long n; if (scanf("%lld", &n) != 1) return 1;
    FILE* f = fopen("enc.in", "w"); fprintf(f, "%lld\n", n); fclose(f);
    if (run("encoder.exe", "enc.in", "enc.out") != 0) { printf("encoder failed\n"); return 0; }
    if (run("decoder.exe", "enc.out", "dec.out") != 0) { printf("decoder failed\n"); return 0; }
    long long m = 0; f = fopen("dec.out", "r");
    if (!f || fscanf(f, "%lld", &m) != 1) { printf("bad decoder output\n"); return 0; }
    fclose(f); printf("%lld\n", m);
}
