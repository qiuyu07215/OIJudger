"""Generate a large benchmark contest.

usage: python scripts/make_bench.py <out_dir> [contestants=120] [cases=10]

4 problems (stdio, file IO, checker-free real, subtasks) x <cases> cases.
Every contestant gets a unique source (a comment with their name), so the
compile cache cannot share binaries. ~85% of solutions are correct, the rest
are WA / RE / TLE so the run includes realistic failures.
"""
import os
import random
import shutil
import sys

out = sys.argv[1]
N = int(sys.argv[2]) if len(sys.argv) > 2 else 120
CASES = int(sys.argv[3]) if len(sys.argv) > 3 else 10
random.seed(42)

if os.path.exists(out):
    shutil.rmtree(out)


def w(rel, text):
    p = os.path.join(out, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w", newline="\n") as f:
        f.write(text)


# Problem "sum": sum of n numbers (stdio). "sortp": sort (file IO). "pre": prefix sums. "mx": max subarray.
for i in range(1, CASES + 1):
    n = 1000 * i
    a = [random.randint(-10**6, 10**6) for _ in range(n)]
    body = f"{n}\n{' '.join(map(str, a))}\n"
    w(f"data/sum/sum{i}.in", body)
    w(f"data/sum/sum{i}.out", f"{sum(a)}\n")
    w(f"data/sortp/sortp{i}.in", body)
    w(f"data/sortp/sortp{i}.out", " ".join(map(str, sorted(a))) + "\n")
    pre, s = [], 0
    for x in a:
        s += x
        pre.append(s)
    w(f"data/pre/pre{i}.in", body)
    w(f"data/pre/pre{i}.out", " ".join(map(str, pre)) + "\n")
    best, cur = a[0], 0
    for x in a:
        cur = max(x, cur + x)
        best = max(best, cur)
    w(f"data/mx/mx{i}.in", body)
    w(f"data/mx/mx{i}.out", f"{best}\n")

subs = lambda p: "\n\n".join(
    f'[[problem.subtask]]\nscore = {100 // CASES}\ntime_ms = 1000\nmemory_mb = 256\ncases = [{{ input = "{p}/{p}{i}.in", output = "{p}/{p}{i}.out" }}]'
    for i in range(1, CASES + 1)
)
w("contest.toml", f'''title = "Benchmark {N}x4x{CASES}"

[[problem]]
name = "sum"
stdio = true

{subs("sum")}

[[problem]]
name = "sortp"

{subs("sortp")}

[[problem]]
name = "pre"
stdio = true

{subs("pre")}

[[problem]]
name = "mx"
stdio = true

{subs("mx")}
''')

READ = "int n; scanf(\"%d\", &n); std::vector<long long> a(n); for (auto& x : a) scanf(\"%lld\", &x);"
GOOD = {
    "sum": "#include <bits/stdc++.h>\nint main() { " + READ + " long long s = 0; for (auto x : a) s += x; printf(\"%lld\\n\", s); }\n",
    "sortp": "#include <bits/stdc++.h>\nint main() { freopen(\"sortp.in\", \"r\", stdin); freopen(\"sortp.out\", \"w\", stdout); " + READ
    + " std::sort(a.begin(), a.end()); for (int i = 0; i < n; i++) printf(\"%lld%c\", a[i], i + 1 < n ? ' ' : '\\n'); }\n",
    "pre": "#include <bits/stdc++.h>\nint main() { " + READ + " long long s = 0; for (int i = 0; i < n; i++) { s += a[i]; printf(\"%lld%c\", s, i + 1 < n ? ' ' : '\\n'); } }\n",
    "mx": "#include <bits/stdc++.h>\nint main() { " + READ + " long long best = a[0], cur = 0; for (auto x : a) { cur = std::max(x, cur + x); best = std::max(best, cur); } printf(\"%lld\\n\", best); }\n",
}
BAD = {
    "wa": "#include <bits/stdc++.h>\nint main() { " + READ + " printf(\"%d\\n\", n); }\n",
    "re": "#include <bits/stdc++.h>\nint main() { " + READ + " volatile int* p = nullptr; *p = n; }\n",
    "tle": "#include <bits/stdc++.h>\nint main() { " + READ + " volatile long long s = 0; for (long long i = 0; i < 4000000000LL; i++) s += i; printf(\"%lld\\n\", (long long)s); }\n",
}

for c in range(1, N + 1):
    name = f"c{c:04d}"
    for p, src in GOOD.items():
        r = random.random()
        if r < 0.85:
            code = src
        elif r < 0.93:
            code = BAD["wa"]
        elif r < 0.98:
            code = BAD["re"]
        else:
            code = BAD["tle"]
        w(f"source/{name}/{p}/{p}.cpp", f"// {name}\n" + code)
print(f"wrote {out}: {N} contestants x 4 problems x {CASES} cases")
