// Baseline: must run normally inside the sandbox.
#include <cstdio>
int main() { long long a, b; if (scanf("%lld%lld", &a, &b) != 2) return 1; printf("%lld\n", a + b); }
