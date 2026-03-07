#!/usr/bin/env python3
"""
Enforce dashboard architecture boundary rules.

This script backs DR-008 and the release gates around:
- direct desktop import ownership
- raw transport bypasses in dashboard code
- unsupported websocket event subscriptions
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "schemas/architecture_guard_policy.json"
DASHBOARD_SRC = ROOT / "dashboard/src"
GATEWAY_WEBSOCKET = ROOT / "crates/ghost-gateway/src/api/websocket.rs"

SOURCE_SUFFIXES = {".ts", ".js", ".svelte"}
IMPORT_RE = re.compile(
    r"""(?:import\s+[^;]*?\s+from\s+|import\s*\()\s*['"]([^'"]+)['"]""",
    re.MULTILINE | re.DOTALL,
)
FETCH_RE = re.compile(r"\bfetch\s*\(")
WEBSOCKET_RE = re.compile(r"new\s+WebSocket\s*\(")
WS_SUBSCRIPTION_RE = re.compile(r"""wsStore\.on\(\s*['"]([A-Za-z0-9_]+)['"]""")
WS_VARIANT_RE = re.compile(r"^\s*([A-Z][A-Za-z0-9_]+)\s*(?:\{|,)")


def load_policy() -> dict[str, object]:
    return json.loads(POLICY.read_text())


def dashboard_files() -> list[Path]:
    return sorted(
        path
        for path in DASHBOARD_SRC.rglob("*")
        if path.is_file() and path.suffix in SOURCE_SUFFIXES
    )


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def is_allowed(rel_path: str, prefixes: list[str] | None = None, paths: list[str] | None = None) -> bool:
    if paths and rel_path in paths:
        return True
    if prefixes:
        return any(rel_path.startswith(prefix) for prefix in prefixes)
    return False


def gateway_ws_events() -> set[str]:
    events: set[str] = set()
    depth = 0
    in_enum = False

    for raw_line in GATEWAY_WEBSOCKET.read_text().splitlines():
        line = raw_line.rstrip()

        if not in_enum:
            if "pub enum WsEvent {" in line:
                in_enum = True
                depth = 1
            continue

        if depth == 1:
            match = WS_VARIANT_RE.match(line)
            if match:
                events.add(match.group(1))

        depth += line.count("{")
        depth -= line.count("}")

        if depth <= 0:
            break

    return events


def dashboard_ws_subscriptions(files: list[Path]) -> dict[str, list[str]]:
    subscriptions: dict[str, list[str]] = {}
    for path in files:
        matches = WS_SUBSCRIPTION_RE.findall(path.read_text())
        if matches:
            subscriptions[rel(path)] = matches
    return subscriptions


def main() -> int:
    policy = load_policy()
    files = dashboard_files()

    runtime_policy = policy["dashboard_runtime_boundary"]
    transport_policy = policy["dashboard_transport_boundary"]

    forbidden_modules = runtime_policy["forbidden_module_tokens"]
    allowed_runtime_prefixes = runtime_policy["allowed_path_prefixes"]
    allowed_fetch_paths = transport_policy["allowed_fetch_paths"]
    allowed_websocket_paths = transport_policy["allowed_websocket_paths"]

    runtime_violations: list[str] = []
    transport_violations: list[str] = []

    for path in files:
        rel_path = rel(path)
        text = path.read_text()

        for module in IMPORT_RE.findall(text):
            if any(token in module for token in forbidden_modules) and not is_allowed(
                rel_path,
                prefixes=allowed_runtime_prefixes,
            ):
                runtime_violations.append(f"{rel_path}: forbidden desktop import `{module}`")

        if FETCH_RE.search(text) and not is_allowed(rel_path, paths=allowed_fetch_paths):
            transport_violations.append(
                f"{rel_path}: raw fetch usage is only allowed in {', '.join(allowed_fetch_paths)}"
            )

        if WEBSOCKET_RE.search(text) and not is_allowed(rel_path, paths=allowed_websocket_paths):
            transport_violations.append(
                f"{rel_path}: raw WebSocket construction is only allowed in {', '.join(allowed_websocket_paths)}"
            )

    gateway_events = gateway_ws_events()
    dashboard_subscriptions = dashboard_ws_subscriptions(files)
    unknown_events: list[str] = []

    for rel_path, events in dashboard_subscriptions.items():
        for event in events:
            if event not in gateway_events:
                unknown_events.append(f"{rel_path}: subscribes to unknown websocket event `{event}`")

    print("Dashboard architecture guard")
    print(f"  dashboard source files scanned: {len(files)}")
    print(f"  gateway websocket events discovered: {len(gateway_events)}")
    print(
        "  dashboard websocket subscriptions discovered: "
        f"{sum(len(events) for events in dashboard_subscriptions.values())}"
    )
    print(f"  runtime boundary violations: {len(runtime_violations)}")
    print(f"  transport boundary violations: {len(transport_violations)}")
    print(f"  websocket event inventory violations: {len(unknown_events)}")

    if runtime_violations:
        print("\nRuntime boundary violations:")
        for violation in runtime_violations:
            print(f"  - {violation}")

    if transport_violations:
        print("\nTransport boundary violations:")
        for violation in transport_violations:
            print(f"  - {violation}")

    if unknown_events:
        print("\nWebSocket event inventory violations:")
        for violation in unknown_events:
            print(f"  - {violation}")

    return 1 if runtime_violations or transport_violations or unknown_events else 0


if __name__ == "__main__":
    sys.exit(main())
