#!/usr/bin/env python3
"""Build pinned libarchive with ZIP-specific boundary corrections.

The private patch preserves ZIP names, validates complete filename fields and
EOCD/directory extents, and searches the full legal comment range without weakening the
strict seekable reader. Installation is prefix-local, not global.
"""

import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import urllib.request


VERSION = "3.8.10"
SHA256 = "56cad765459d5ee25a3b761f90c47d99379d28297fbfb72d908880d6b875fb96"
URL = (
    "https://github.com/libarchive/libarchive/releases/download/"
    f"v{VERSION}/libarchive-{VERSION}.tar.xz"
)


def build(prefix: Path) -> None:
    prefix = prefix.resolve()
    if prefix == Path("/") or prefix == Path("/usr") or prefix == Path("/usr/local"):
        raise ValueError("choose a private dependency prefix, not a system prefix")
    with tempfile.TemporaryDirectory(prefix="nacre-libarchive-") as temporary:
        work = Path(temporary)
        package = work / "libarchive.tar.xz"
        digest = hashlib.sha256()
        with urllib.request.urlopen(URL, timeout=60) as response, package.open("wb") as output:
            while chunk := response.read(262144):
                digest.update(chunk)
                output.write(chunk)
        if digest.hexdigest() != SHA256:
            raise RuntimeError("libarchive release SHA-256 does not match the pinned digest")
        with tarfile.open(package) as archive:
            archive.extractall(work, filter="data")
        source = work / f"libarchive-{VERSION}"
        patch = Path(__file__).with_name("libarchive-zip-boundary.patch").resolve()
        subprocess.run(
            ["patch", "--batch", "--forward", "--fuzz=0", "-p1", "-i", str(patch)],
            cwd=source, check=True,
        )
        binary = work / "build"
        subprocess.run(
            [
                "cmake", "-S", str(source), "-B", str(binary),
                f"-DCMAKE_INSTALL_PREFIX={prefix}", "-DCMAKE_INSTALL_LIBDIR=lib",
                "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=ON",
                "-DENABLE_TEST=OFF", "-DENABLE_TAR=OFF", "-DENABLE_CPIO=OFF",
                "-DENABLE_CAT=OFF", "-DENABLE_UNZIP=OFF",
            ],
            check=True,
        )
        subprocess.run(
            ["cmake", "--build", str(binary), "--parallel", str(os.cpu_count() or 1)],
            check=True,
        )
        subprocess.run(["cmake", "--install", str(binary)], check=True)
    print(f"Installed corrected libarchive in {prefix}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    args = parser.parse_args()
    build(args.prefix)


if __name__ == "__main__":
    main()
