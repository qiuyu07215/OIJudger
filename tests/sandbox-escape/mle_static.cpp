// 1 GiB static array under a 256 MiB limit: must be MLE.
#include <cstdio>
static char big[1u << 30];
int main() { for (unsigned i = 0; i < sizeof big; i += 4096) big[i] = 1; printf("%d\n", big[12345]); }
