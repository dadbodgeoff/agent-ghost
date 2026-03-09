#!/usr/bin/env python3
"""
Check parity between gateway WebSocket event variants and the SDK event union.

This guard exists to prevent the Rust `WsEvent` enum from drifting away from
`packages/sdk/src/websocket.ts`, which is the dashboard's typed contract.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GATEWAY_WEBSOCKET = ROOT / "crates/ghost-gateway/src/api/websocket.rs"
SDK_WEBSOCKET = ROOT / "packages/sdk/src/websocket.ts"

RUST_VARIANT_RE = re.compile(r"^\s*([A-Z][A-Za-z0-9_]+)\s*(?:\{|,)")
SDK_EVENT_BLOCK_RE = re.compile(
    r"export type KnownWsEvent =(?P<body>.*?)\nexport interface UnknownWsEvent",
    re.DOTALL,
)
SDK_EVENT_TYPE_RE = re.compile(r"type:\s*'([A-Za-z][A-Za-z0-9_]*)'")


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
            match = RUST_VARIANT_RE.match(line)
            if match:
                events.add(match.group(1))

        depth += line.count("{")
        depth -= line.count("}")

        if depth <= 0:
            break

    return events


def sdk_ws_events() -> set[str]:
    text = SDK_WEBSOCKET.read_text()
    match = SDK_EVENT_BLOCK_RE.search(text)
    if not match:
        raise RuntimeError("Could not locate KnownWsEvent in packages/sdk/src/websocket.ts")
    return set(SDK_EVENT_TYPE_RE.findall(match.group("body")))


def main() -> int:
    gateway_events = gateway_ws_events()
    sdk_events = sdk_ws_events()

    missing_in_sdk = sorted(gateway_events - sdk_events)
    extra_in_sdk = sorted(sdk_events - gateway_events)

    print("WebSocket contract parity report")
    print(f"  gateway websocket events: {len(gateway_events)}")
    print(f"  sdk websocket events: {len(sdk_events)}")
    print(f"  missing in sdk: {len(missing_in_sdk)}")
    print(f"  extra in sdk: {len(extra_in_sdk)}")

    if missing_in_sdk:
        print("\nGateway events missing from SDK:")
        for event in missing_in_sdk:
            print(f"  - {event}")

    if extra_in_sdk:
        print("\nSDK events missing from gateway:")
        for event in extra_in_sdk:
            print(f"  - {event}")

    return 1 if missing_in_sdk or extra_in_sdk else 0


if __name__ == "__main__":
    sys.exit(main())
