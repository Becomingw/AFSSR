#!/usr/bin/env python3
"""Snapshot the complete dirty checkout without ignored local credentials/artifacts."""
import argparse
import hashlib
import json
import pathlib
import subprocess
import tarfile


def snapshot(root, destination):
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root
    ).decode().split("\0")
    files = []
    blocked = {".git", "node_modules", "target", ".codex", ".agents", "__pycache__"}
    for name in sorted(set(names) - {""}):
        relative = pathlib.PurePosixPath(name)
        if relative.is_absolute() or ".." in relative.parts or blocked.intersection(relative.parts):
            raise RuntimeError("Unsafe source path")
        path = root / name
        if not path.exists():
            continue
        if path.is_symlink():
            raise RuntimeError("Review symlinks before snapshot: " + name)
        if not path.is_file():
            continue
        if path.name == ".env" or (path.name.startswith(".env.") and not path.name.endswith("example")):
            raise RuntimeError("Credential-like environment file in source list")
        files.append({"path": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("xb") as output:
        with tarfile.open(fileobj=output, mode="w:gz") as archive:
            for file in files:
                archive.add(root / file["path"], arcname=file["path"], recursive=False)
    manifest = {
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        "sha256": hashlib.sha256(destination.read_bytes()).hexdigest(),
        "source_tree_sha256": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
        "files": files,
    }
    destination.chmod(0o600)
    destination.with_suffix(destination.suffix + ".manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({key: value for key, value in manifest.items() if key != "files"} | {"file_count": len(files), "bytes": destination.stat().st_size}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=pathlib.Path)
    parser.add_argument("destination", type=pathlib.Path)
    args = parser.parse_args()
    snapshot(args.root.resolve(), args.destination.resolve())
