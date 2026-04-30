#!/usr/bin/env python3
from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def check_toml(path: Path) -> list[str]:
    errors = []
    try:
        with path.open("rb") as fh:
            tomllib.load(fh)
    except Exception as exc:  # pragma: no cover
        errors.append(f"TOML inválido em {path}: {exc}")
    return errors


def check_balanced_delimiters(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    text = rust_code_without_comments_and_literals(text)
    errors = []
    pairs = [("{", "}"), ("(", ")"), ("[", "]")]
    for left, right in pairs:
        if text.count(left) != text.count(right):
            errors.append(
                f"Delimitadores desbalanceados em {path}: {left}={text.count(left)} {right}={text.count(right)}"
            )
    return errors


def rust_code_without_comments_and_literals(text: str) -> str:
    output: list[str] = []
    index = 0
    block_comment_depth = 0
    state = "code"
    raw_hashes = 0
    escaped = False

    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""

        if state == "code":
            raw = raw_string_prefix_at(text, index)
            if raw is not None:
                prefix_len, raw_hashes = raw
                output.extend(" " for _ in range(prefix_len))
                index += prefix_len
                state = "raw_string"
                continue

            if char == "/" and next_char == "/":
                output.extend("  ")
                index += 2
                state = "line_comment"
                continue
            if char == "/" and next_char == "*":
                output.extend("  ")
                index += 2
                block_comment_depth = 1
                state = "block_comment"
                continue
            if char == '"':
                output.append(" ")
                index += 1
                escaped = False
                state = "string"
                continue
            if char == "'" and looks_like_char_literal_start(text, index):
                output.append(" ")
                index += 1
                escaped = False
                state = "char"
                continue

            output.append(char)
            index += 1
            continue

        if state == "line_comment":
            output.append("\n" if char == "\n" else " ")
            index += 1
            if char == "\n":
                state = "code"
            continue

        if state == "block_comment":
            output.append("\n" if char == "\n" else " ")
            if char == "/" and next_char == "*":
                output.append(" ")
                index += 2
                block_comment_depth += 1
                continue
            if char == "*" and next_char == "/":
                output.append(" ")
                index += 2
                block_comment_depth -= 1
                if block_comment_depth == 0:
                    state = "code"
                continue
            index += 1
            continue

        if state == "string":
            output.append("\n" if char == "\n" else " ")
            index += 1
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                state = "code"
            continue

        if state == "char":
            output.append("\n" if char == "\n" else " ")
            index += 1
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == "'":
                state = "code"
            continue

        if state == "raw_string":
            output.append("\n" if char == "\n" else " ")
            if char == '"' and text.startswith("#" * raw_hashes, index + 1):
                output.extend(" " for _ in range(raw_hashes))
                index += 1 + raw_hashes
                state = "code"
                raw_hashes = 0
                continue
            index += 1
            continue

    return "".join(output)


def raw_string_prefix_at(text: str, index: int) -> tuple[int, int] | None:
    prefixes = ("r", "br")
    for prefix in prefixes:
        if not text.startswith(prefix, index):
            continue
        cursor = index + len(prefix)
        hashes = 0
        while cursor < len(text) and text[cursor] == "#":
            hashes += 1
            cursor += 1
        if cursor < len(text) and text[cursor] == '"':
            return cursor - index + 1, hashes
    return None


def looks_like_char_literal_start(text: str, index: int) -> bool:
    previous = text[index - 1] if index > 0 else ""
    if previous.isalnum() or previous in "_":
        return False
    cursor = index + 1
    escaped = False
    while cursor < len(text) and cursor <= index + 8:
        char = text[cursor]
        if char == "\n":
            return False
        if escaped:
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == "'":
            return True
        cursor += 1
    return False


def main() -> int:
    errors: list[str] = []
    for path in [ROOT / "Cargo.toml", ROOT / "examples" / "config.toml"]:
        errors.extend(check_toml(path))
    for path in ROOT.rglob("*.rs"):
        errors.extend(check_balanced_delimiters(path))

    if errors:
        for error in errors:
            print(error)
        return 1

    print("Verificação estática concluída sem erros estruturais óbvios.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
