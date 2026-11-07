// Allocates and touches memory until it fails: must be MLE.
#include <cstdlib>
#include <cstring>
int main() { for (;;) { char* p = (char*)malloc(1 << 20); if (!p) return 2; memset(p, 1, 1 << 20); } }
