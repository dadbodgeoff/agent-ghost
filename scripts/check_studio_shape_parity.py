#!/usr/bin/env python3
"""
Enforce typed OpenAPI contracts for the Studio critical-path endpoints.

This guard is intentionally narrow: it protects the Studio session/message
surface from regressing back to `inline(serde_json::Value)` helpers or dropping
public path/query parameters without updating the contract.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OPENAPI = ROOT / "crates/ghost-gateway/src/api/openapi.rs"

OPENAPI_HELPER_RE = re.compile(
    r"#\[utoipa::path\((?P<body>.*?)\)\]\s*async fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*\(",
    re.MULTILINE | re.DOTALL,
)
PATH_RE = re.compile(r'path\s*=\s*"([^"]+)"')
METHOD_RE = re.compile(r"^\s*(get|post|put|delete|patch)\s*,", re.MULTILINE)
PARAM_NAME_RE = re.compile(r'\("([A-Za-z_][A-Za-z0-9_]*)"\s*=')
PARAMS_BLOCK_RE = re.compile(
    r"params\((?P<body>.*?)\)\s*,\s*(?:request_body|responses)",
    re.DOTALL,
)
REQUEST_BODY_RE = re.compile(r"request_body\s*=\s*([A-Za-z0-9_:]+)")
RESPONSE_BODY_RE = re.compile(r"body\s*=\s*([A-Za-z0-9_:]+)")


EXPECTATIONS: dict[tuple[str, str], dict[str, object]] = {
    ("get", "/api/studio/sessions"): {
        "params": {"limit", "cursor"},
        "request_body": None,
        "response_schemas": {"StudioSessionListResponseSchema"},
    },
    ("post", "/api/studio/sessions"): {
        "params": set(),
        "request_body": "StudioCreateSessionRequestSchema",
        "response_schemas": {"StudioSessionSchema"},
    },
    ("get", "/api/studio/sessions/{id}"): {
        "params": {"id"},
        "request_body": None,
        "response_schemas": {"StudioSessionWithMessagesResponseSchema"},
    },
    ("delete", "/api/studio/sessions/{id}"): {
        "params": {"id"},
        "request_body": None,
        "response_schemas": {"StudioDeleteSessionResponseSchema"},
    },
    ("post", "/api/studio/sessions/{id}/messages"): {
        "params": {"id"},
        "request_body": "StudioSendMessageRequestSchema",
        "response_schemas": {
            "StudioSendMessageResponseSchema",
            "StudioMessageAcceptedResponseSchema",
        },
    },
    ("post", "/api/studio/sessions/{id}/messages/stream"): {
        "params": {"id"},
        "request_body": "StudioSendMessageRequestSchema",
        "response_schemas": set(),
    },
    ("get", "/api/studio/sessions/{id}/stream/recover"): {
        "params": {"id", "message_id", "after_seq"},
        "request_body": None,
        "response_schemas": {"StudioRecoverStreamResponseSchema"},
    },
    ("post", "/api/studio/run"): {
        "params": set(),
        "request_body": "crate::api::studio::StudioRunRequest",
        "response_schemas": {"crate::api::studio::StudioRunResponse"},
    },
}


def read_helpers() -> dict[tuple[str, str], str]:
    helpers: dict[tuple[str, str], str] = {}
    text = OPENAPI.read_text()

    for match in OPENAPI_HELPER_RE.finditer(text):
        body = match.group("body")
        path_match = PATH_RE.search(body)
        method_match = METHOD_RE.search(body)
        if not path_match or not method_match:
            continue
        helpers[(method_match.group(1), path_match.group(1))] = body

    return helpers


def main() -> int:
    helpers = read_helpers()
    violations: list[str] = []

    for key, expected in EXPECTATIONS.items():
        body = helpers.get(key)
        if body is None:
            violations.append(f"missing OpenAPI helper for {key[0].upper()} {key[1]}")
            continue

        if "inline(serde_json::Value)" in body:
            violations.append(f"{key[0].upper()} {key[1]} uses inline(serde_json::Value)")

        params_block = PARAMS_BLOCK_RE.search(body)
        actual_params = set()
        if params_block:
            actual_params = set(PARAM_NAME_RE.findall(params_block.group("body")))
        expected_params = expected["params"]
        if actual_params != expected_params:
            violations.append(
                f"{key[0].upper()} {key[1]} params drifted: expected {sorted(expected_params)}, got {sorted(actual_params)}"
            )

        actual_request_body = None
        request_body_match = REQUEST_BODY_RE.search(body)
        if request_body_match:
            actual_request_body = request_body_match.group(1)

        expected_request_body = expected["request_body"]
        if actual_request_body != expected_request_body:
            violations.append(
                f"{key[0].upper()} {key[1]} request body drifted: expected {expected_request_body}, got {actual_request_body}"
            )

        response_schemas = set(RESPONSE_BODY_RE.findall(body))
        missing_responses = sorted(expected["response_schemas"] - response_schemas)
        if missing_responses:
            violations.append(
                f"{key[0].upper()} {key[1]} is missing response schemas {missing_responses}"
            )

    print("Studio shape parity report")
    print(f"  helpers checked: {len(EXPECTATIONS)}")
    print(f"  violations: {len(violations)}")

    if violations:
        print("\nViolations:")
        for violation in violations:
            print(f"  - {violation}")

    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
