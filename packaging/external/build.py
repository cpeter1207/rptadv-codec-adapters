"""Build hash-pinned external shared backends in an explicit staging prefix.

No codec implementation is vendored. The temporary upstream tree is removed
after installation; the installed libraries remain separate dynamic objects.
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def verify(archive, expected):
    """Reject any release archive not matching the reviewed source pin."""
    with archive.open("rb") as stream:
        actual = hashlib.file_digest(stream, "sha256").hexdigest()
    if actual != expected:
        raise ValueError("external source hash mismatch")


def archive_for(record, supplied, cache):
    """Reuse only verified source bytes; publish downloads atomically to the cache."""
    if supplied is not None:
        verify(supplied, record["sha256"])
        return supplied
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / (record["sha256"] + ".tar.gz")
    if not archive.exists():
        with tempfile.NamedTemporaryFile(dir=cache, delete=False) as target:
            temporary = Path(target.name)
            try:
                with urllib.request.urlopen(record["url"], timeout=60) as response:
                    shutil.copyfileobj(response, target)
                target.flush()
                verify(temporary, record["sha256"])
                target.close()
                temporary.replace(archive)
            finally:
                target.close()
                temporary.unlink(missing_ok=True)
    verify(archive, record["sha256"])
    return archive


def main():
    """Download or verify one release, build it, and stage its shared artifacts."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("backend", choices=["bcg729", "ilbc", "dialogic"])
    parser.add_argument("--prefix", required=True, type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument(
        "--cache",
        type=Path,
        default=Path(
            os.environ.get("RPTADV_CODEC_SOURCE_CACHE", ROOT / ".work/external-sources")
        ),
    )
    parser.add_argument("--jobs", type=int, default=1)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("jobs must be positive")
    record = next(
        item
        for item in json.loads((ROOT / "dependencies/codecs.json").read_text())[
            "sources"
        ]
        if item["backend"] == args.backend
    )
    work = ROOT / "build/external"
    work.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f"{args.backend}.", dir=work) as temporary:
        temporary = Path(temporary)
        archive = archive_for(record, args.archive, args.cache)
        source = temporary / "source"
        source.mkdir()
        with tarfile.open(archive) as release:
            release.extractall(source, filter="data")
        (upstream,) = source.iterdir()
        recipe = Path(__file__).resolve().parent / args.backend
        if args.backend == "dialogic":
            original = (upstream / "codecs/codec_adpcm.c").read_text()
            license_text = original[: original.index("/*! \\file")]
            primitive = original[
                original.index("static int indsft") : original.index(
                    "/*----------------- Asterisk-codec glue"
                )
            ]
            # Identical bounded arithmetic without C's undefined negative shift.
            old = "return state->signal << 4;"
            if primitive.count(old) != 1:
                raise ValueError("released Dialogic extraction boundary changed")
            primitive = primitive.replace(old, "return state->signal * 16;")
            (temporary / "released_adpcm.inc").write_text(license_text + primitive)
        build = temporary / "build"
        subprocess.run(
            [
                "cmake",
                "-S",
                str(recipe),
                "-B",
                str(build),
                f"-DUPSTREAM_SOURCE_DIR={upstream}",
                f"-DGENERATED_DIR={temporary}",
                f"-DCMAKE_INSTALL_PREFIX={args.prefix.resolve()}",
                "-DCMAKE_INSTALL_LIBDIR=lib",
                "-DCMAKE_BUILD_TYPE=Release",
                "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
            ],
            check=True,
        )
        subprocess.run(
            ["cmake", "--build", str(build), "--parallel", str(args.jobs)], check=True
        )
        subprocess.run(["cmake", "--install", str(build)], check=True)
        notices = args.prefix / "share/doc" / f"rptadv-{args.backend}"
        notices.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(upstream / "COPYING", notices / "copyright.upstream")


if __name__ == "__main__":
    main()
