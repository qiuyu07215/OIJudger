// Tries to reach the network: loopback listener (port in argv[1]), a public
// IP, and DNS resolution. Prints one line per probe and finally BLOCKED if
// every attempt failed, LEAK otherwise.
#include <winsock2.h>
#include <ws2tcpip.h>
#include <cstdio>
#include <cstdlib>
static bool try_connect(const char* ip, int port) {
    SOCKET s = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (s == INVALID_SOCKET) return false;
    u_long nb = 1; ioctlsocket(s, FIONBIO, &nb);
    sockaddr_in a{}; a.sin_family = AF_INET; a.sin_port = htons(port); inet_pton(AF_INET, ip, &a.sin_addr);
    bool ok = connect(s, (sockaddr*)&a, sizeof a) == 0;
    if (!ok && WSAGetLastError() == WSAEWOULDBLOCK) {
        fd_set w, e; FD_ZERO(&w); FD_ZERO(&e); FD_SET(s, &w); FD_SET(s, &e);
        timeval tv{2, 0};
        ok = select(0, nullptr, &w, &e, &tv) > 0 && FD_ISSET(s, &w);
    }
    closesocket(s);
    return ok;
}
int main(int argc, char** argv) {
    WSADATA w; if (WSAStartup(MAKEWORD(2, 2), &w) != 0) { puts("wsastartup failed\nBLOCKED"); return 0; }
    bool leak = false, r;
    if (argc > 1) { r = try_connect("127.0.0.1", atoi(argv[1])); printf("loopback %s\n", r ? "LEAK" : "blocked"); leak |= r; }
    r = try_connect("1.1.1.1", 53); printf("internet %s\n", r ? "LEAK" : "blocked"); leak |= r;
    addrinfo* ai = nullptr; r = getaddrinfo("example.com", "80", nullptr, &ai) == 0 && ai;
    if (ai) freeaddrinfo(ai);
    printf("dns %s\n", r ? "LEAK" : "blocked"); leak |= r;
    puts(leak ? "LEAK" : "BLOCKED");
}
