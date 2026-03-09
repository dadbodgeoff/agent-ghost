#!/usr/bin/env python3
"""
Verify that the checked-in SDK generated types match the current OpenAPI output.

This guard fails when `packages/sdk/src/generated-types.ts` is stale relative to
the gateway's current `openapi-dump` output.
"""

from __future__ import annotations

import difflib
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SDK_ROOT = ROOT / "packages/sdk"
GENERATED_TYPES = SDK_ROOT / "src/generated-types.ts"


def run(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        text=True,
        capture_output=True,
        check=False,
    )


def require_executable(name: str) -> None:
    if shutil.which(name) is None:
        raise RuntimeError(f"Required executable `{name}` was not found on PATH")


def normalize(text: str) -> str:
    return text.replace("\r\n", "\n")


def main() -> int:
    require_executable("cargo")
    require_executable("pnpm")

    dump = run(
        ["cargo", "run", "--quiet", "--bin", "ghost", "--", "openapi-dump"],
        ROOT,
    )
    if dump.returncode != 0:
        print("Failed to dump the current OpenAPI spec from the ghost binary.", file=sys.stderr)
        if dump.stderr:
            print(dump.stderr, file=sys.stderr, end="")
        return dump.returncode or 1

    current = normalize(GENERATED_TYPES.read_text())

    with tempfile.TemporaryDirectory(prefix="ghost-openapi-") as tempdir:
        tempdir_path = Path(tempdir)
        spec_path = tempdir_path / "openapi.json"
        generated_path = tempdir_path / "generated-types.ts"
        spec_path.write_text(dump.stdout)

        generated = run(
            [
                "pnpm",
                "--dir",
                str(SDK_ROOT),
                "exec",
                "openapi-typescript",
                str(spec_path),
                "-o",
                str(generated_path),
            ],
            ROOT,
        )
        if generated.returncode != 0:
            print("Failed to generate SDK types from the dumped OpenAPI spec.", file=sys.stderr)
            if generated.stderr:
                print(generated.stderr, file=sys.stderr, end="")
            return generated.returncode or 1

        fresh = normalize(generated_path.read_text())

    if current != fresh:
        print("Generated types freshness check failed: packages/sdk/src/generated-types.ts is stale.")
        diff = difflib.unified_diff(
            current.splitlines(),
            fresh.splitlines(),
            fromfile="checked-in/generated-types.ts",
            tofile="fresh/generated-types.ts",
            n=3,
            lineterm="",
        )
        for index, line in enumerate(diff):
            if index >= 200:
                print("... diff truncated ...")
                break
            print(line)
        return 1

    print("Generated types freshness check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
