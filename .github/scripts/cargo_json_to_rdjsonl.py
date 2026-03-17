#!/usr/bin/env python3

import argparse
import json
import sys


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Convert cargo JSON diagnostics to reviewdog rdjsonl."
    )
    parser.add_argument(
        "--tool-name",
        default="clippy",
        help="Label used in reviewdog messages.",
    )
    return parser.parse_args()


def severity_for(level: str) -> str | None:
    mapping = {
        "error": "ERROR",
        "warning": "WARNING",
    }
    return mapping.get(level)


def first_primary_span(message: dict) -> dict | None:
    spans = message.get("spans", [])
    for span in spans:
        if span.get("is_primary"):
            return span

    if spans:
        return spans[0]

    for child in message.get("children", []):
        for span in child.get("spans", []):
            if span.get("is_primary"):
                return span

    return None


def build_message(tool_name: str, diagnostic: dict) -> str:
    code = diagnostic.get("code") or {}
    code_value = code.get("code")
    if code_value:
        return f"[{tool_name}:{code_value}] {diagnostic['message']}"
    return f"[{tool_name}] {diagnostic['message']}"


def emit_rdjsonl(tool_name: str, payload: dict) -> None:
    if payload.get("reason") != "compiler-message":
        return

    diagnostic = payload.get("message") or {}
    severity = severity_for(diagnostic.get("level", ""))
    if severity is None:
        return

    span = first_primary_span(diagnostic)
    if not span:
        return

    path = span.get("file_name")
    if not path or path.startswith("<"):
        return

    start_line = span.get("line_start")
    start_column = span.get("column_start")
    end_line = span.get("line_end") or start_line
    end_column = span.get("column_end") or start_column
    if not start_line or not start_column:
        return

    reviewdog_diagnostic = {
        "message": build_message(tool_name, diagnostic),
        "severity": severity,
        "location": {
            "path": path,
            "range": {
                "start": {
                    "line": start_line,
                    "column": start_column,
                },
                "end": {
                    "line": end_line,
                    "column": end_column,
                },
            },
        },
    }

    print(json.dumps(reviewdog_diagnostic, ensure_ascii=False))


def main() -> int:
    args = parse_args()

    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue

        try:
            payload = json.loads(raw_line)
        except json.JSONDecodeError:
            continue

        emit_rdjsonl(args.tool_name, payload)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
