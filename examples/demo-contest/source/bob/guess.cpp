#include <cstdio>
int main() {
    for (long long g = 1;; g++) {
        printf("%lld\n", g); fflush(stdout);
        char r[4]; if (scanf("%3s", r) != 1) return 0;
        if (r[0] == '=') break;
    }
}
