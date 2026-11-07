// Writes forever: must be OLE.
#include <cstdio>
int main() { static char buf[1 << 16]; for (auto& c : buf) c = 'x'; for (;;) fwrite(buf, 1, sizeof buf, stdout); }
