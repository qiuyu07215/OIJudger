// Baseline: file IO in the working directory must work.
#include <cstdio>
int main() {
    freopen("sum.in", "r", stdin); freopen("sum.out", "w", stdout);
    long long a, b; scanf("%lld%lld", &a, &b); printf("%lld\n", a + b);
}
