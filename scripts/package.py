"""Build release archives.

usage: python scripts/package.py <target_dir> [mingw_dir]

<target_dir>  cargo target directory containing release/OIJudger.exe and
              release/oijudger-cli.exe
[mingw_dir]   MinGW installation to bundle in the portable archive
              (default C:\\mingw64; skipped if missing)

Writes dist/OIJudger-<ver>-win64.zip and dist/OIJudger-<ver>-win64-portable.zip.

The portable archive bundles MinGW, including every runtime DLL that judged
programs may load (RUNTIME_DLLS in crates/judge-core/src/compile.rs: programs
built without -static, with -fopenmp, prebuilt checkers, ...). Judged programs
get mingw64/bin on their PATH, so the DLLs must be there.
"""
import os
import re
import shutil
import sys
import zipfile

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
target = sys.argv[1]
mingw = sys.argv[2] if len(sys.argv) > 2 else r"C:\mingw64"
version = re.search(r'^version = "([^"]+)"', open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8").read(), re.M).group(1)
dist = os.path.join(ROOT, "dist")
name = f"OIJudger-{version}-win64"
stage = os.path.join(dist, name)

if os.path.exists(stage):
    shutil.rmtree(stage)
os.makedirs(stage)
for exe in ["OIJudger.exe", "oijudger-cli.exe"]:
    shutil.copy2(os.path.join(target, "release", exe), stage)
# GNU builds load WebView2Loader.dll dynamically (MSVC builds link it in).
loader = os.path.join(target, "release", "WebView2Loader.dll")
with open(os.path.join(target, "release", "OIJudger.exe"), "rb") as f:
    needs_loader = b"WebView2Loader.dll" in f.read()
if needs_loader:
    if not os.path.isfile(loader):
        sys.exit("OIJudger.exe needs WebView2Loader.dll but it is not in the target directory")
    shutil.copy2(loader, stage)
shutil.copy2(os.path.join(ROOT, "README.md"), stage)
shutil.copy2(os.path.join(ROOT, "LICENSE"), os.path.join(stage, "LICENSE.txt"))
os.makedirs(os.path.join(stage, "licenses"), exist_ok=True)
shutil.copy2(os.path.join(ROOT, "assets", "testlib", "LICENSE"), os.path.join(stage, "licenses", "testlib-LICENSE.txt"))
shutil.copytree(
    os.path.join(ROOT, "examples", "demo-contest"),
    os.path.join(stage, "examples", "demo-contest"),
    ignore=shutil.ignore_patterns(".oij", "results.*"),
)


def zip_dir(src, out, extra=None):
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for base, prefix in [(src, name)] + (extra or []):
            for d, _, files in os.walk(base):
                for f in files:
                    p = os.path.join(d, f)
                    z.write(p, os.path.join(prefix, os.path.relpath(p, base)))
    print(f"wrote {out} ({os.path.getsize(out) / 1e6:.1f} MB)")


zip_dir(stage, os.path.join(dist, f"{name}.zip"))

def runtime_dlls():
    src = open(os.path.join(ROOT, "crates", "judge-core", "src", "compile.rs"), encoding="utf-8").read()
    block = re.search(r"pub const RUNTIME_DLLS: \[&str; \d+\] = \[(.*?)\];", src, re.S).group(1)
    return re.findall(r'"([^"]+)"', block)


if os.path.isdir(mingw):
    bundled = os.path.join(dist, "mingw64-stage")
    if os.path.exists(bundled):
        shutil.rmtree(bundled)
    # opt/ only holds gdb's Python runtime: not needed to compile.
    shutil.copytree(mingw, bundled, ignore=shutil.ignore_patterns("opt"))
    bundled_bin = os.path.join(bundled, "bin")
    # Some MinGW builds keep runtime DLLs only next to the libraries; judged
    # programs look in bin (their PATH), so make sure each one is there.
    lib_dirs = [os.path.join(bundled, "x86_64-w64-mingw32", "lib"), os.path.join(bundled, "x86_64-w64-mingw32", "bin")]
    missing = []
    for dll in runtime_dlls():
        if os.path.isfile(os.path.join(bundled_bin, dll)):
            continue
        found = next((os.path.join(d, dll) for d in lib_dirs if os.path.isfile(os.path.join(d, dll))), None)
        if found:
            shutil.copy2(found, bundled_bin)
        else:
            missing.append(dll)
    if missing:
        sys.exit(f"MinGW in {mingw} lacks runtime DLLs: {', '.join(missing)}")
    dlls = sorted(f for f in os.listdir(bundled_bin) if f.lower().endswith(".dll"))
    print(f"bundled runtime DLLs in mingw64/bin: {', '.join(dlls)}")
    zip_dir(stage, os.path.join(dist, f"{name}-portable.zip"), [(bundled, os.path.join(name, "mingw64"))])
    shutil.rmtree(bundled)
else:
    print(f"{mingw} not found: portable archive skipped")
