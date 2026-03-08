#!/usr/bin/env python3
"""
Check parity between mounted gateway routes and documented OpenAPI paths.

This script exists to support DR-001 and RG-01. It is intentionally lightweight:
it extracts string routes from `build_router()` and path declarations from
`crates/ghost-gateway/src/api/openapi.rs`, applies an explicit exclusion policy,
and reports drift.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OPENAPI = ROOT / "crates/ghost-gateway/src/api/openapi.rs"
POLICY = ROOT / "schemas/openapi_parity_policy.json"
ROUTE_SOURCES = [
    ROOT / "crates/ghost-gateway/src/route_sets.rs",
    ROOT / "crates/ghost-gateway/src/api/push_routes.rs",
    ROOT / "crates/ghost-gateway/src/api/mesh_routes.rs",
]

ROUTE_RE = re.compile(r'\.route\(\s*"([^"]+)"', re.MULTILINE)
OPENAPI_HELPER_RE = re.compile(
    r"#\[utoipa::path\((?P<body>.*?)\)\]\s*async fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*\(",
    re.MULTILINE | re.DOTALL,
)
OPENAPI_PATH_RE = re.compile(r'path\s*=\s*"([^"]+)"')
OPENAPI_LIST_RE = re.compile(r"paths\((?P<body>.*?)\),\s*components", re.MULTILINE | re.DOTALL)
COLON_PARAM_RE = re.compile(r":([A-Za-z_][A-Za-z0-9_]*)")


def normalize_router_path(path: str) -> str:
    return COLON_PARAM_RE.sub(r"{\1}", path)


def load_policy() -> dict[str, dict[str, str]]:
    raw = json.loads(POLICY.read_text())
    return {
        "excluded_router_paths": raw.get("excluded_router_paths", {}),
        "excluded_spec_paths": raw.get("excluded_spec_paths", {}),
    }


def read_router_paths() -> set[str]:
    paths: set[str] = set()
    for route_source in ROUTE_SOURCES:
        text = route_source.read_text()
        paths.update(
            normalize_router_path(match.group(1))
            for match in ROUTE_RE.finditer(text)
        )
    return paths


def read_listed_helper_names() -> list[str]:
    text = OPENAPI.read_text()
    match = OPENAPI_LIST_RE.search(text)
    if not match:
        raise RuntimeError("Could not locate ApiDoc paths(...) list in openapi.rs")

    names: list[str] = []
    for line in match.group("body").splitlines():
        entry = line.strip().rstrip(",")
        if not entry or entry.startswith("//"):
            continue
        names.append(entry)
    return names


def read_defined_helpers() -> dict[str, str]:
    text = OPENAPI.read_text()
    helpers: dict[str, str] = {}

    for match in OPENAPI_HELPER_RE.finditer(text):
        body = match.group("body")
        path_match = OPENAPI_PATH_RE.search(body)
        if not path_match:
            continue
        helpers[match.group("name")] = path_match.group(1)

    return helpers


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--fail-on-drift",
        action="store_true",
        help="Exit non-zero when undocumented or stale schema paths remain.",
    )
    args = parser.parse_args()

    policy = load_policy()
    router_paths = read_router_paths()
    listed_helpers = read_listed_helper_names()
    defined_helpers = read_defined_helpers()

    missing_helper_defs = sorted(
        helper for helper in listed_helpers if helper not in defined_helpers
    )
    unreferenced_helper_defs = sorted(
        helper for helper in defined_helpers if helper not in set(listed_helpers)
    )
    spec_paths = {
        defined_helpers[helper]
        for helper in listed_helpers
        if helper in defined_helpers
    }

    excluded_router = set(policy["excluded_router_paths"])
    excluded_spec = set(policy["excluded_spec_paths"])

    undocumented = sorted(router_paths - spec_paths - excluded_router)
    stale = sorted(spec_paths - router_paths - excluded_spec)
    covered = sorted((router_paths & spec_paths) | excluded_router)

    print("OpenAPI parity report")
    print(f"  mounted routes: {len(router_paths)}")
    print(f"  documented paths: {len(spec_paths)}")
    print(f"  covered or intentionally excluded routes: {len(covered)}")
    print(f"  undocumented mounted routes: {len(undocumented)}")
    print(f"  stale documented paths: {len(stale)}")
    print(f"  missing helper definitions: {len(missing_helper_defs)}")
    print(f"  unreferenced helper definitions: {len(unreferenced_helper_defs)}")

    if missing_helper_defs:
        print("\nListed OpenAPI helpers without a matching definition:")
        for helper in missing_helper_defs:
            print(f"  - {helper}")

    if unreferenced_helper_defs:
        print("\nDefined OpenAPI helpers not included in ApiDoc:")
        for helper in unreferenced_helper_defs:
            print(f"  - {helper}")

    if undocumented:
      print("\nUndocumented mounted routes:")
      for path in undocumented:
          print(f"  - {path}")

    if stale:
      print("\nStale documented paths:")
      for path in stale:
          print(f"  - {path}")

    if policy["excluded_router_paths"]:
      print("\nIntentional router exclusions:")
      for path, reason in sorted(policy["excluded_router_paths"].items()):
          print(f"  - {path}: {reason}")

    if policy["excluded_spec_paths"]:
      print("\nIntentional spec exclusions:")
      for path, reason in sorted(policy["excluded_spec_paths"].items()):
          print(f"  - {path}: {reason}")

    if args.fail_on_drift and (
        undocumented or stale or missing_helper_defs or unreferenced_helper_defs
    ):
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
