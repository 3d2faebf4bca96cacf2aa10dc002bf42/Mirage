#!/usr/bin/env python3
"""Check publishable source policies without displaying suspected secret values.

Use --inventory for metadata-only counts of all local files, including ignored
artifacts. Symlinks and junctions are not traversed. This is a policy check, not
a substitute for Gitleaks, dependency audits, prose review, or history review.
"""
from __future__ import annotations

import argparse
from collections import Counter
import os
from pathlib import Path
import re
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def is_link(path: Path) -> bool:
    """Detect symlinks and Windows reparse points, including on Python 3.10/3.11."""
    metadata = path.lstat()
    return stat.S_ISLNK(metadata.st_mode) or bool(
        getattr(metadata, "st_file_attributes", 0)
        & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    )


def inventory() -> int:
    counts: Counter[str] = Counter()
    sizes: Counter[str] = Counter()
    skipped = 0
    errors = []

    def on_error(error: OSError) -> None:
        errors.append(type(error).__name__)

    for base, directories, files in os.walk(ROOT, followlinks=False, onerror=on_error):
        for name in directories[:]:
            path = Path(base) / name
            try:
                if is_link(path):
                    directories.remove(name)
                    skipped += 1
            except OSError as error:
                directories.remove(name)
                on_error(error)
        for name in files:
            path = Path(base) / name
            group = path.relative_to(ROOT).parts[0]
            try:
                if is_link(path):
                    skipped += 1
                    continue
                sizes[group] += path.stat(follow_symlinks=False).st_size
                counts[group] += 1
            except OSError as error:
                on_error(error)
    for group in sorted(counts):
        print(f"{group}: {counts[group]} files, {sizes[group]} bytes")
    print(f"Skipped links: {skipped}; inaccessible entries: {len(errors)}")
    return int(bool(errors))


def check() -> int:
    result = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=ROOT, check=True, capture_output=True,
    )
    paths = sorted(set(result.stdout.decode("utf-8").split("\0")) - {""})
    errors = []
    inspected = 0
    # Only locations and rule names are reported; matched content stays private.
    rules = {
        "personal Windows path": re.compile(r"(?i)[A-Z]:\\Users\\(?!appveyor(?:\\|/))[^\\/\s]+"),
        "personal Unix path": re.compile(r"/(?:home|Users)/(?!appveyor/)[A-Za-z0-9_.-]+/"),
        "private key material": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
        "GitHub token": re.compile(r"gh[pousr]_[A-Za-z0-9]{30,}"),
    }
    language = re.compile(r"\b(?:n\u00e3o|voc\u00ea|conectando|recebidos|enviados|apelido|aguardando)\b", re.IGNORECASE)
    for name in paths:
        path = ROOT / name
        try:
            linked = is_link(path)
        except FileNotFoundError:
            continue  # git may still list an intentionally deleted file
        except OSError:
            errors.append((name, "inaccessible file"))
            continue
        if linked or not path.resolve().is_relative_to(ROOT):
            errors.append((name, "link or path outside repository"))
            continue
        if path.name.startswith(".env") and path.name != ".env.example":
            errors.append((name, "private environment file"))
            continue
        if path.name == "identity.json" or path.suffix.lower() in {".pfx", ".p12", ".pcap", ".pcapng", ".sqlitedb"}:
            errors.append((name, "private runtime artifact"))
            continue
        if path.stat().st_size > 2_000_000:
            errors.append((name, "large publishable file requires manual review"))
            continue
        try:
            source = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            errors.append((name, "binary/non-UTF-8 file requires manual review"))
            continue
        inspected += 1
        for rule, pattern in rules.items():
            if pattern.search(source):
                errors.append((name, rule))
        if not name.startswith("client/vendor/") and name != "scripts/check_repository.py":
            if language.search(source) or 'lang="pt-BR"' in source:
                errors.append((name, "known English-only policy regression"))
        if name.startswith(".github/workflows/"):
            if "pull_request_target" in source:
                errors.append((name, "privileged PR trigger"))
            for action in re.findall(r"uses:\s*([^\s#]+)", source):
                if not action.startswith("./") and not re.fullmatch(r"[^@]+@[0-9a-f]{40}", action):
                    errors.append((name, "action must use a full commit SHA"))
    for name, rule in errors:
        print(f"FAIL {name}: {rule}")
    print(f"Inspected {inspected} publishable UTF-8 files; {len(errors)} policy findings.")
    print("Phrase checks are incomplete; manually review English prose and secret-scan the source and history.")
    return int(bool(errors))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", action="store_true", help="count local files without opening content")
    args = parser.parse_args()
    sys.exit(inventory() if args.inventory else check())
