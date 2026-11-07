"""Build release archives.

usage: python scripts/package.py <target_dir> [mingw_dir]

<target_dir>  cargo target directory containing release/OIJudger.exe and
              release/oijudger-cli.exe
[mingw_dir]   MinGW installation to bundle in the portable archive
              (default C:\\mingw64; skipped if missing)

Writes dist/OIJudger-<ver>-win64.zip and dist/OIJudger-<ver>-win64-portable.zip.
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
shutil.copy2(os.path.join(ROOT, "README.md"), stage)
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

if os.path.isdir(mingw):
    bundled = os.path.join(dist, "mingw64-stage")
    if os.path.exists(bundled):
        shutil.rmtree(bundled)
    # opt/ only holds gdb's Python runtime: not needed to compile.
    shutil.copytree(mingw, bundled, ignore=shutil.ignore_patterns("opt"))
    zip_dir(stage, os.path.join(dist, f"{name}-portable.zip"), [(bundled, os.path.join(name, "mingw64"))])
    shutil.rmtree(bundled)
else:
    print(f"{mingw} not found: portable archive skipped")
