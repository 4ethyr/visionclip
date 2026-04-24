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
    errors = []
    pairs = [("{", "}"), ("(", ")"), ("[", "]")]
    for left, right in pairs:
        if text.count(left) != text.count(right):
            errors.append(
                f"Delimitadores desbalanceados em {path}: {left}={text.count(left)} {right}={text.count(right)}"
            )
    return errors


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
