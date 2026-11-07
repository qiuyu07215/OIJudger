// Integer division by zero: must be a runtime error.
#include <cstdio>
int main() { volatile int z = 0; printf("%d\n", 10 / z); }
