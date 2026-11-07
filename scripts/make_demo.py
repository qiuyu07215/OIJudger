"""Generate examples/demo-contest: every problem type and most verdicts.

Expected scores are written to expected.json:
alice 700, bob 310, eve 100, carol 0, dave 0.
"""
import json
import os
import shutil

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "examples", "demo-contest")


def w(rel, text):
    p = os.path.join(ROOT, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w", newline="\n", encoding="utf-8") as f:
        f.write(text)


if os.path.exists(ROOT):
    shutil.rmtree(ROOT)

# ---------------- data ----------------
for i, (a, b) in enumerate([(1, 2), (100, 200), (2000000000, 2000000000), (-5, 3)], 1):
    w(f"data/sum/sum{i}.in", f"{a} {b}\n")
    w(f"data/sum/sum{i}.out", f"{a + b}\n")
for i, n in enumerate([10, 100000], 1):
    w(f"data/fio/fio{i}.in", f"{n}\n")
    w(f"data/fio/fio{i}.out", f"{n * (n + 1) // 2}\n")
for i, (n, r) in enumerate([(2, "1.41421356237"), (10, "3.16227766017")], 1):
    w(f"data/real/real{i}.in", f"{n}\n")
    w(f"data/real/real{i}.ans", f"{r}\n")
w("data/real/checker.cpp", r'''#include "testlib.h"
#include <cmath>
int main(int argc, char* argv[]) {
    registerTestlibCmd(argc, argv);
    double j = ans.readDouble(), p = ouf.readDouble();
    double e = std::fabs(j - p);
    if (e <= 1e-6) quitf(_ok, "exact");
    if (e <= 1e-2) quitp(0.5, "close, error %.3g", e);
    quitf(_wa, "expected %.6f, found %.6f", j, p);
}
''')
for i, s in enumerate([123456789, 7], 1):
    w(f"data/guess/guess{i}.in", f"{s}\n")
    w(f"data/guess/guess{i}.ans", "0\n")
w("data/guess/interactor.cpp", r'''#include "testlib.h"
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
''')
for i, (q, a) in enumerate([("3", "9"), ("12", "144")], 1):
    w(f"data/ans/ans{i}.in", q + "\n")
    w(f"data/ans/ans{i}.out", a + "\n")
for i, (a, b) in enumerate([(1, 2), (-1000000000000, 5)], 1):
    w(f"data/add/add{i}.in", f"{a} {b}\n")
    w(f"data/add/add{i}.out", f"{a + b}\n")
for i, n in enumerate([5, -123456789012], 1):
    w(f"data/relay/relay{i}.in", f"{n}\n")
    w(f"data/relay/relay{i}.out", f"{n}\n")
w("data/relay/grader.cpp", r'''// Starts the contestant's encoder.exe and decoder.exe and relays data.
#include <windows.h>
#include <cstdio>
static int run(const char* exe, const char* in, const char* out) {
    SECURITY_ATTRIBUTES sa{sizeof sa, nullptr, TRUE};
    HANDLE hi = CreateFileA(in, GENERIC_READ, FILE_SHARE_READ, &sa, OPEN_EXISTING, 0, nullptr);
    HANDLE ho = CreateFileA(out, GENERIC_WRITE, 0, &sa, CREATE_ALWAYS, 0, nullptr);
    if (hi == INVALID_HANDLE_VALUE || ho == INVALID_HANDLE_VALUE) return -1;
    STARTUPINFOA si{}; si.cb = sizeof si; si.dwFlags = STARTF_USESTDHANDLES;
    si.hStdInput = hi; si.hStdOutput = ho; si.hStdError = GetStdHandle(STD_ERROR_HANDLE);
    PROCESS_INFORMATION pi{};
    char cmd[64]; snprintf(cmd, sizeof cmd, "%s", exe);
    if (!CreateProcessA(exe, cmd, nullptr, nullptr, TRUE, 0, nullptr, nullptr, &si, &pi)) {
        CloseHandle(hi); CloseHandle(ho); return -2;
    }
    WaitForSingleObject(pi.hProcess, INFINITE);
    DWORD code = 1; GetExitCodeProcess(pi.hProcess, &code);
    CloseHandle(pi.hProcess); CloseHandle(pi.hThread); CloseHandle(hi); CloseHandle(ho);
    return (int)code;
}
int main() {
    long long n; if (scanf("%lld", &n) != 1) return 1;
    FILE* f = fopen("enc.in", "w"); fprintf(f, "%lld\n", n); fclose(f);
    if (run("encoder.exe", "enc.in", "enc.out") != 0) { printf("encoder failed\n"); return 0; }
    if (run("decoder.exe", "enc.out", "dec.out") != 0) { printf("decoder failed\n"); return 0; }
    long long m = 0; f = fopen("dec.out", "r");
    if (!f || fscanf(f, "%lld", &m) != 1) { printf("bad decoder output\n"); return 0; }
    fclose(f); printf("%lld\n", m);
}
''')
w("data/add/add.h", "long long add(long long a, long long b);\n")
w("data/add/grader.cpp", r'''#include <cstdio>
#include "add.h"
int main() { long long a, b; scanf("%lld%lld", &a, &b); printf("%lld\n", add(a, b)); }
''')


def subtasks(prefix, pairs, scores, deps=None):
    out = []
    for i, (cases, score) in enumerate(zip(pairs, scores)):
        lines = ["[[problem.subtask]]", f"score = {score}", "time_ms = 1000", "memory_mb = 256"]
        if deps and deps[i]:
            lines.append(f"depends = {deps[i]}")
        cs = ", ".join(f'{{ input = "{prefix}/{a}", output = "{prefix}/{b}" }}' for a, b in cases)
        lines.append(f"cases = [{cs}]")
        out.append("\n".join(lines))
    return "\n\n".join(out)


w("contest.toml", "\n\n".join([
    'title = "OIJudger Demo"',
    '[[problem]]\nname = "sum"\ntitle = "A + B"\nstdio = true',
    subtasks("sum", [[("sum1.in", "sum1.out")], [("sum2.in", "sum2.out")],
                     [("sum3.in", "sum3.out"), ("sum4.in", "sum4.out")]], [30, 30, 40], [None, [1], [2]]),
    '[[problem]]\nname = "fio"\ntitle = "File IO"',
    subtasks("fio", [[("fio1.in", "fio1.out")], [("fio2.in", "fio2.out")]], [50, 50]),
    '[[problem]]\nname = "real"\ntitle = "Square root (testlib checker)"\nstdio = true\n'
    'checker = { path = "real/checker.cpp", kind = "testlib" }',
    subtasks("real", [[("real1.in", "real1.ans")], [("real2.in", "real2.ans")]], [50, 50]),
    '[[problem]]\nname = "guess"\ntitle = "Guess the number (interactor)"\nkind = "interactive"\n'
    'interactor = "guess/interactor.cpp"',
    subtasks("guess", [[("guess1.in", "guess1.ans")], [("guess2.in", "guess2.ans")]], [50, 50]),
    '[[problem]]\nname = "ans"\ntitle = "Squares (answer submission)"\nkind = "answer"',
    subtasks("ans", [[("ans1.in", "ans1.out")], [("ans2.in", "ans2.out")]], [50, 50]),
    '[[problem]]\nname = "add"\ntitle = "Add (NOI-style grader)"\nkind = "interactive"\nstdio = true\n'
    'grader = { header_path = "add/add.h", header_name = "add.h", grader_path = "add/grader.cpp" }',
    subtasks("add", [[("add1.in", "add1.out")], [("add2.in", "add2.out")]], [50, 50]),
    '[[problem]]\nname = "relay"\ntitle = "Relay (multi-program communication)"\nkind = "communication"\nstdio = true\n'
    'communication = { sources = ["encoder", "decoder"], interfaces = ["relay/grader.cpp"], multi_program = true }',
    subtasks("relay", [[("relay1.in", "relay1.out")], [("relay2.in", "relay2.out")]], [50, 50]),
]) + "\n")

# ---------------- sources ----------------
SUM_OK = r'''#include <cstdio>
int main() { long long a, b; scanf("%lld%lld", &a, &b); printf("%lld\n", a + b); }
'''
FIO_OK = r'''#include <cstdio>
int main() {
    freopen("fio.in", "r", stdin); freopen("fio.out", "w", stdout);
    long long n; scanf("%lld", &n); printf("%lld\n", n * (n + 1) / 2);
}
'''
REAL_OK = r'''#include <cstdio>
#include <cmath>
int main() { double n; scanf("%lf", &n); printf("%.9f\n", sqrt(n)); }
'''
GUESS_OK = r'''#include <cstdio>
int main() {
    long long lo = 1, hi = 1000000000;
    while (true) {
        long long m = (lo + hi) / 2;
        printf("%lld\n", m); fflush(stdout);
        char r[4]; if (scanf("%3s", r) != 1) return 0;
        if (r[0] == '=') break;
        if (r[0] == '<') lo = m + 1; else hi = m - 1;
    }
}
'''
ADD_OK = '#include "add.h"\nlong long add(long long a, long long b) { return a + b; }\n'

# alice: everything correct, problem subfolders
w("source/alice/sum/sum.cpp", SUM_OK)
w("source/alice/fio/fio.cpp", FIO_OK)
w("source/alice/real/real.cpp", REAL_OK)
w("source/alice/guess/guess.cpp", GUESS_OK)
w("source/alice/ans/ans1.out", "9\n")
w("source/alice/ans/ans2.out", "144\n")
w("source/alice/add/add.cpp", ADD_OK)
ENCODER = '#include <cstdio>\nint main() { long long x; scanf("%lld", &x); printf("%lld\\n", 2 * x + 7); }\n'
w("source/alice/relay/encoder.cpp", ENCODER)
w("source/alice/relay/decoder.cpp", '#include <cstdio>\nint main() { long long y; scanf("%lld", &y); printf("%lld\\n", (y - 7) / 2); }\n')

# bob: flat layout, partial results
w("source/bob/sum.cpp", r'''#include <cstdio>
int main() { int a, b; scanf("%d%d", &a, &b); printf("%d\n", a + b); }
''')
w("source/bob/fio.cpp", r'''#include <cstdio>
int main() { long long n; scanf("%lld", &n); printf("%lld\n", n * (n + 1) / 2); }
''')
w("source/bob/real.cpp", r'''#include <cstdio>
#include <cmath>
int main() { double n; scanf("%lf", &n); printf("%.2f\n", sqrt(n)); }
''')
w("source/bob/guess.cpp", r'''#include <cstdio>
int main() {
    for (long long g = 1;; g++) {
        printf("%lld\n", g); fflush(stdout);
        char r[4]; if (scanf("%3s", r) != 1) return 0;
        if (r[0] == '=') break;
    }
}
''')
w("source/bob/ans1.out", "9\n")
w("source/bob/add.cpp", ADD_OK)
w("source/bob/encoder.cpp", ENCODER)
w("source/bob/decoder.cpp", '#include <cstdio>\nint main() { long long y; scanf("%lld", &y); printf("%lld\\n", y); }\n')

# carol: compile errors and resource failures
w("source/carol/sum.cpp", "int main() { this is not c++ }\n")
w("source/carol/fio.cpp", "int main() { volatile unsigned long long x = 0; for (;;) x++; }\n")
w("source/carol/real.cpp", r'''#include <vector>
#include <cstdio>
int main() { std::vector<int> v(1000000000, 1); printf("%d\n", v[123]); }
''')
# (1 / z would be optimised by GCC into a branch and not crash)
w("source/carol/guess.cpp", "int main() { *(volatile int*)0 = 1; }\n")
w("source/carol/add.cpp", '#include "add.h"\n')

# dave: nothing submitted
os.makedirs(os.path.join(ROOT, "source", "dave"), exist_ok=True)
w("source/dave/.keep", "")

# eve: misbehaves (network, child process, data snooping) but answers correctly
w("source/eve/sum.cpp", r'''#include <windows.h>
#include <cstdio>
#include <cstdlib>
// Tries the network (through a dynamically loaded winsock), starts a child
// process and snoops for answer files, then answers correctly anyway.
typedef int (WINAPI *startup_t)(WORD, void*);
typedef UINT_PTR (WINAPI *socket_t)(int, int, int);
typedef int (WINAPI *connect_t)(UINT_PTR, const void*, int);
int main() {
    HMODULE ws = LoadLibraryA("ws2_32.dll");
    if (ws) {
        char wsa[512];
        ((startup_t)GetProcAddress(ws, "WSAStartup"))(0x0202, wsa);
        UINT_PTR s = ((socket_t)GetProcAddress(ws, "socket"))(2, 1, 6);
        unsigned char addr[16] = {2, 0, 0, 80, 1, 1, 1, 1};
        ((connect_t)GetProcAddress(ws, "connect"))(s, addr, 16);
    }
    system("calc.exe");
    FILE* f = fopen("../../../../data/sum/sum1.out", "r"); if (f) fclose(f);
    long long x, y; scanf("%lld%lld", &x, &y); printf("%lld\n", x + y);
}
''')

w("expected.json", json.dumps({
    "alice": {"sum": 100, "fio": 100, "real": 100, "guess": 100, "ans": 100, "add": 100, "relay": 100},
    "bob": {"sum": 60, "fio": 0, "real": 50, "guess": 50, "ans": 50, "add": 100, "relay": 0},
    "carol": {"sum": 0, "fio": 0, "real": 0, "guess": 0, "ans": 0, "add": 0, "relay": 0},
    "dave": {"sum": 0, "fio": 0, "real": 0, "guess": 0, "ans": 0, "add": 0, "relay": 0},
    "eve": {"sum": 100, "fio": 0, "real": 0, "guess": 0, "ans": 0, "add": 0, "relay": 0},
}, indent=2) + "\n")
print("demo contest written to", os.path.normpath(ROOT))
