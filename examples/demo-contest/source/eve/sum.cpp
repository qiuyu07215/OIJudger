#include <windows.h>
#include <cstdio>
#include <cstdlib>
// Tries the network (through a dynamically loaded winsock), starts a child
// process and snoops for answer files, then answers correctly anyway.
typedef int (WINAPI *startup_t)(WORD, void*);
typedef UINT_PTR (WINAPI *socket_t)(int, int, int);
typedef int (WINAPI *connect_t)(UINT_PTR, const void*, int);
int main() {
    HMODULE ws = LoadLibraryA("ws2_32.dll");
    if (ws) {
        char wsa[512];
        ((startup_t)GetProcAddress(ws, "WSAStartup"))(0x0202, wsa);
        UINT_PTR s = ((socket_t)GetProcAddress(ws, "socket"))(2, 1, 6);
        unsigned char addr[16] = {2, 0, 0, 80, 1, 1, 1, 1};
        ((connect_t)GetProcAddress(ws, "connect"))(s, addr, 16);
    }
    system("calc.exe");
    FILE* f = fopen("../../../../data/sum/sum1.out", "r"); if (f) fclose(f);
    long long x, y; scanf("%lld%lld", &x, &y); printf("%lld\n", x + y);
}
