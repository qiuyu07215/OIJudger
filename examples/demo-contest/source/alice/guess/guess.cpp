#include <cstdio>
int main() {
    long long lo = 1, hi = 1000000000;
    while (true) {
        long long m = (lo + hi) / 2;
        printf("%lld\n", m); fflush(stdout);
        char r[4]; if (scanf("%3s", r) != 1) return 0;
        if (r[0] == '=') break;
        if (r[0] == '<') lo = m + 1; else hi = m - 1;
    }
}
