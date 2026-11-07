#include "testlib.h"
#include <cmath>
int main(int argc, char* argv[]) {
    registerTestlibCmd(argc, argv);
    double j = ans.readDouble(), p = ouf.readDouble();
    double e = std::fabs(j - p);
    if (e <= 1e-6) quitf(_ok, "exact");
    if (e <= 1e-2) quitp(0.5, "close, error %.3g", e);
    quitf(_wa, "expected %.6f, found %.6f", j, p);
}
