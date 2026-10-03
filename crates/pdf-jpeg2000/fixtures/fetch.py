"""Fetch and verify the immutable upstream JPEG 2000 test corpus."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from urllib.request import urlopen


ROOT = Path(__file__).resolve().parent
MANIFEST = ROOT / "manifest.json"
CACHE = ROOT / "cache"


def load_manifest():
    with MANIFEST.open(encoding="utf-8") as source:
        return json.load(source)


def asset_path(asset):
    path = Path(asset["path"])
    if path.is_absolute() or not path.parts or any(part in (".", "..") for part in path.parts):
        raise ValueError(f"unsafe asset path: {asset['path']}")
    return CACHE / asset["source"] / path


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_file(path, expected_hash, expected_size):
    if not path.is_file():
        raise FileNotFoundError(path)
    if path.stat().st_size != expected_size or sha256(path) != expected_hash:
        raise ValueError(f"fixture checksum mismatch: {path}")


def fetch_asset(asset, source):
    destination = asset_path(asset)
    if destination.is_file():
        verify_file(destination, asset["sha256"], asset["bytes"])
        return

    destination.parent.mkdir(parents=True, exist_ok=True)
    url = f"{source['raw_base']}/{source['revision']}/{asset['path']}"
    with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as temporary:
        temporary_path = Path(temporary.name)
        try:
            received = 0
            with urlopen(url, timeout=30) as response:
                for block in iter(lambda: response.read(1024 * 1024), b""):
                    received += len(block)
                    if received > asset["bytes"]:
                        raise ValueError(f"fixture exceeds recorded size: {url}")
                    temporary.write(block)
            temporary.flush()
            verify_file(temporary_path, asset["sha256"], asset["bytes"])
            os.replace(temporary_path, destination)
        finally:
            temporary_path.unlink(missing_ok=True)


def build_pdf(manifest):
    derived = manifest["derived_pdf"]
    path = CACHE / derived["path"]
    if path.is_file():
        verify_file(path, derived["sha256"], derived["bytes"])
        return

    script = CACHE / "pdfium" / "testing/tools/fixup_pdf_template.py"
    template = CACHE / "pdfium" / "testing/resources/pixel/jpxdecode.in"
    subprocess.run([sys.executable, str(script), str(template)], check=True)
    verify_file(path, derived["sha256"], derived["bytes"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("fetch", "verify"))
    args = parser.parse_args()
    manifest = load_manifest()

    for asset in manifest["assets"]:
        if args.action == "fetch":
            fetch_asset(asset, manifest["sources"][asset["source"]])
        else:
            verify_file(asset_path(asset), asset["sha256"], asset["bytes"])

    if args.action == "fetch":
        build_pdf(manifest)
    else:
        derived = manifest["derived_pdf"]
        verify_file(CACHE / derived["path"], derived["sha256"], derived["bytes"])

    print(f"Verified {len(manifest['assets'])} upstream assets and the PDF fixture")


if __name__ == "__main__":
    main()
