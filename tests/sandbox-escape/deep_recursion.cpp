// ~200 MiB of stack. Must pass with a 512 MiB limit once the PE stack
// reserve is patched; crashes with the default 2 MiB stack.
#include <cstdio>
int depth(int n) { volatile char pad[96]; pad[0] = (char)n; if (n == 0) return pad[0]; return depth(n - 1) + pad[0] % 2; }
int main() { printf("%d\n", depth(1600000)); }
