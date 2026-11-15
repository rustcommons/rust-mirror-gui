#!/usr/bin/env python3
"""Package one release binary as <name>-<version>-<target>.<zip|tar.gz>.

The archive has one top-level folder of the same name containing the
executable, LICENSE and README.txt (shared layout with rust-mirror). Entries
get a fixed timestamp and permissions, so packaging the same files gives the
same archive. Python 3 standard library only; runs on Windows, Linux and macOS.

Usage: package.py --binary PATH --license PATH --name NAME --version X.Y.Z
                  --target TRIPLE --out DIR [--glibc 2.NN]
Prints the archive path.
"""
import argparse
import gzip
import io
import pathlib
import tarfile
import time
import zipfile

FIXED_TIME = (2026, 1, 1, 0, 0, 0)

README = """{name} {version} ({target})
{underline}

A desktop app for building offline Rust toolchain and crate mirrors with
rust-mirror. Source, documentation and release notes:
https://github.com/rustcommons/rust-mirror-gui
Release notes for this version:
https://github.com/rustcommons/rust-mirror-gui/blob/v{version}/docs/releases/v{version}.md
Built from: {source}

Run it
------
{run}

The program is NOT code-signed.
{unsigned}

Check the download
------------------
Compare the archive's SHA-256 with the SHA256SUMS file on the release page:
  Windows (PowerShell):  Get-FileHash {archive}
  Linux:                 sha256sum -c --ignore-missing SHA256SUMS
  macOS:                 shasum -a 256 -c --ignore-missing SHA256SUMS
With the GitHub CLI you can also check where it was built:
  gh attestation verify {archive} --repo rustcommons/rust-mirror-gui

License: Apache-2.0 (see LICENSE).
"""

RUN = {
    "windows": """Unzip the folder anywhere and double-click rust-mirror-gui.exe.
Windows 10 or 11, or Windows Server 2022 or newer. Nothing else to install:
the C runtime is built in, so no Visual C++ Redistributable is needed.""",
    "linux": """Extract the folder and run ./rust-mirror-gui from it.
It needs glibc {glibc} or newer (its highest GLIBC_ symbol version), for
example RHEL, AlmaLinux or Rocky Linux 8, Ubuntu 20.04, Debian 10, SLES 15 SP4 or
newer. It was built on AlmaLinux 8 (glibc {glibc_built}) and tested to start
on AlmaLinux 8 and Ubuntu 20.04.
The window needs X11 or Wayland, a Vulkan or OpenGL (EGL) driver,
xkbcommon and a system font for symbols such as the check mark. Desktop distributions already have them. On a minimal or
GPU-less machine, install:
  Debian/Ubuntu:  sudo apt install libxkbcommon-x11-0 libvulkan1 mesa-vulkan-drivers libegl1 libgl1-mesa-dri libxcursor1 libxrandr2 libxi6 fonts-dejavu-core fontconfig
  RHEL family:    sudo dnf install libxkbcommon-x11 vulkan-loader mesa-vulkan-drivers mesa-libEGL mesa-dri-drivers libXcursor libXrandr libXi dejavu-sans-fonts fontconfig""",
    "macos": """Extract the folder and run ./rust-mirror-gui from Terminal.
macOS 11 or newer ({arch}).""",
}

UNSIGNED = {
    "windows": """Windows SmartScreen may say "Windows protected your PC". Check the
SHA-256 first (below), then choose "More info" and "Run anyway". Some
company policies block unsigned programs; ask IT to allow this one.""",
    "linux": """Linux does not ask about signatures; check the SHA-256 below.""",
    "macos": """macOS Gatekeeper may say the app "cannot be opened because the
developer cannot be verified". Check the SHA-256 first (below), then either
right-click the program in Finder and choose Open, or run once in Terminal:
  xattr -d com.apple.quarantine ./rust-mirror-gui""",
}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for a in ("binary", "license", "name", "version", "target", "out"):
        p.add_argument("--" + a, required=True)
    p.add_argument("--glibc", default="", help="highest GLIBC symbol the binary needs (Linux)")
    p.add_argument("--glibc-built", default="", help="glibc of the build runner (Linux)")
    p.add_argument("--source", default="", help="tag and commit the binary was built from")
    args = p.parse_args()
    target = args.target
    family = "windows" if "windows" in target else "macos" if "apple" in target else "linux"
    ext = "zip" if family == "windows" else "tar.gz"
    base = f"{args.name}-{args.version}-{target}"
    archive = f"{base}.{ext}"
    exe = args.name + (".exe" if family == "windows" else "")
    title = f"{args.name} {args.version} ({target})"
    readme = README.format(
        name=args.name,
        version=args.version,
        target=target,
        underline="=" * len(title),
        run=RUN[family].format(glibc=args.glibc or "?", glibc_built=args.glibc_built or "2.35", arch="Apple silicon" if target.startswith("aarch64") else "Intel"),
        source=args.source or "(not recorded)",
        unsigned=UNSIGNED[family],
        archive=archive,
    )
    if family == "windows":
        readme = readme.replace("\n", "\r\n")
    files = [
        (exe, pathlib.Path(args.binary).read_bytes(), 0o755),
        ("LICENSE", pathlib.Path(args.license).read_bytes(), 0o644),
        ("README.txt", readme.encode("utf-8"), 0o644),
    ]
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    path = out / archive
    if family == "windows":
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
            for name, data, mode in files:
                info = zipfile.ZipInfo(f"{base}/{name}", FIXED_TIME)
                info.external_attr = (0o100000 | mode) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                z.writestr(info, data)
    else:
        mtime = time.mktime(FIXED_TIME + (0, 0, -1))
        # gzip with a fixed header time and no file name, so it is repeatable.
        with open(path, "wb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9) as gz, tarfile.open(fileobj=gz, mode="w", format=tarfile.PAX_FORMAT) as t:
            d = tarfile.TarInfo(base)
            d.type, d.mode, d.mtime = tarfile.DIRTYPE, 0o755, mtime
            t.addfile(d)
            for name, data, mode in files:
                info = tarfile.TarInfo(f"{base}/{name}")
                info.size, info.mode, info.mtime = len(data), mode, mtime
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                t.addfile(info, io.BytesIO(data))
    print(path)


if __name__ == "__main__":
    main()
