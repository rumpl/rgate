#!/usr/bin/env python3
"""Collect resolved dependency notices; flag crates lacking packaged license texts."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
PREFIXES = ("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE")


def notice_files(root):
    # Include native bundled-library notices, but don't mistake copyright.svg
    # or Rust source files for standalone notices.
    return sorted(
        path for path in root.rglob("*")
        if path.is_file()
        and path.name.upper().startswith(PREFIXES)
        and path.suffix.lower() not in (".rs", ".svg", ".png", ".html", ".c", ".h")
        and "target" not in path.relative_to(root).parts
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--metadata", type=Path, help="Use an existing Cargo metadata JSON")
    args = parser.parse_args()
    if args.metadata:
        metadata = json.loads(args.metadata.read_text())
    else:
        command = ["cargo", "metadata", "--locked", "--all-features", "--format-version", "1"]
        metadata = json.loads(subprocess.check_output(command, cwd=ROOT, text=True))
    workspace = set(metadata["workspace_members"])
    packages = sorted(
        (p for p in metadata["packages"] if p["id"] not in workspace),
        key=lambda p: (p["name"], p["version"], p["source"] or ""),
    )
    records = []
    sections = [
        "RGate third-party dependency license inventory\n"
        "Includes all resolved platforms/features and build/test dependencies;\n"
        "not every listed package is linked into every distribution.\n"
        "License expressions are retained as declared, not blanket relicensing.\n"
        "See NOTICE for artwork/fonts/HDL and source-distribution obligations.\n"
    ]
    missing = []
    for package in packages:
        root = Path(package["manifest_path"]).parent
        files = notice_files(root)
        texts = []
        for path in files:
            relative = str(path.relative_to(root))
            content = path.read_text(errors="replace")
            texts.append({"path": relative, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
            sections.append(f"\n--- {package['name']} {package['version']} / {relative} ---\n{content}\n")
        record = {
            "name": package["name"], "version": package["version"],
            "license": package["license"], "source": package["source"],
            "repository": package["repository"], "authors": package["authors"],
            "license_files": texts,
            "package_source": f"https://crates.io/crates/{package['name']}/{package['version']}"
            if (package["source"] or "").startswith("registry+") else package["source"],
        }
        sections.append(
            f"\nPackage: {package['name']} {package['version']}\n"
            f"Declared license: {package['license'] or 'UNDECLARED'}\n"
            f"Source: {record['package_source']}\nRepository: {package['repository']}\n"
        )
        if not files:
            missing.append(f"{package['name']} {package['version']}")
            sections.append("REVIEW: no standalone license/notice text found in the Cargo package.\n")
            for path in [root / "Cargo.toml.orig", root / "README.md", root / "README"]:
                if path.is_file():
                    sections.append(f"\n--- {path.name} ---\n{path.read_text(errors='replace')}\n")
        records.append(record)
    args.output.mkdir(parents=True, exist_ok=True)
    for path in (ROOT / "licenses").iterdir():
        if path.is_file():
            shutil.copy2(path, args.output / path.name)
    for path in (ROOT / "assets/fonts").glob("*LICENSE*"):
        shutil.copy2(path, args.output / path.name)
    for directory in ["projectf", "uart", "picorv32"]:
        name = "COPYING" if directory == "picorv32" else "LICENSE"
        shutil.copy2(ROOT / "examples/verilog" / directory / name, args.output / f"HDL-{directory}-{name}")
    (args.output / "THIRD-PARTY-LICENSES.txt").write_text("\n".join(sections))
    (args.output / "THIRD-PARTY-PACKAGES.json").write_text(json.dumps(records, indent=2) + "\n")
    (args.output / "LICENSE-REVIEW.txt").write_text(
        "Packages without standalone license texts; verify upstream terms before release:\n"
        + "\n".join(missing) + "\n"
    )
    print(f"Collected {len(records)} dependency records; {len(missing)} require license-text review.")


if __name__ == "__main__":
    main()
