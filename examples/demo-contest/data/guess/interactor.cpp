#include "testlib.h"
#include <iostream>
int main(int argc, char* argv[]) {
    registerInteraction(argc, argv);
    long long secret = inf.readLong();
    for (int q = 1;; q++) {
        long long g = ouf.readLong();
        if (q > 40) quitf(_wa, "too many queries");
        if (g == secret) {
            std::cout << "=" << std::endl;
            tout << q << std::endl;
            quitf(_ok, "found in %d queries", q);
        }
        std::cout << (g < secret ? "<" : ">") << std::endl;
    }
}
