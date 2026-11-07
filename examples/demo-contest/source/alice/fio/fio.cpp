#include <cstdio>
int main() {
    freopen("fio.in", "r", stdin); freopen("fio.out", "w", stdout);
    long long n; scanf("%lld", &n); printf("%lld\n", n * (n + 1) / 2);
}
