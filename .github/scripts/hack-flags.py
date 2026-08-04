#!/usr/bin/env python3
"""Translate a crate's hack.toml into cargo-hack flags.

Usage: hack-flags.py <crate-dir>

Prints the flags on one line. A crate without a hack.toml gets the default
pairwise powerset. Feature names are validated against the crate's Cargo.toml
so a renamed or misspelled entry fails CI instead of silently widening the
powerset.
"""

import sys
import tomllib
from pathlib import Path

# hack.toml key -> cargo-hack flag. List values repeat the flag per entry.
LIST_KEYS = {
    "skip": "--exclude-features",
    "group": "--group-features",
    "mutually-exclusive": "--mutually-exclusive-features",
    "at-least-one-of": "--at-least-one-of",
}
SCALAR_KEYS = {"depth": "--depth"}
FLAG_KEYS = {
    "exclude-no-default-features": "--exclude-no-default-features",
    "exclude-all-features": "--exclude-all-features",
}
KNOWN = set(LIST_KEYS) | set(SCALAR_KEYS) | set(FLAG_KEYS)

DEFAULT_DEPTH = 2


def die(msg: str) -> None:
    print(f"hack-flags: {msg}", file=sys.stderr)
    sys.exit(1)


def declared_features(crate: Path) -> set[str]:
    """Every feature name the crate can be built with."""
    manifest = crate / "Cargo.toml"
    if not manifest.is_file():
        die(f"{manifest} not found")
    with manifest.open("rb") as fh:
        cargo = tomllib.load(fh)

    names = set(cargo.get("features", {}))
    # An optional dependency implies a feature of the same name unless the
    # manifest already declares one via dep: syntax.
    for section in ("dependencies", "build-dependencies"):
        for name, spec in cargo.get(section, {}).items():
            if isinstance(spec, dict) and spec.get("optional"):
                names.add(name)
    return names


def normalise(key: str, value: object) -> list[list[str]]:
    """Return each flag occurrence as its own list of feature names."""
    if not isinstance(value, list):
        die(f"`{key}` must be an array")
    if all(isinstance(item, str) for item in value):
        # A flat array is one occurrence for group-like keys, but `skip` is
        # naturally flat and cargo-hack accepts it comma-joined either way.
        return [list(value)]
    groups = []
    for item in value:
        if not isinstance(item, list) or not all(isinstance(f, str) for f in item):
            die(f"`{key}` must be an array of strings or an array of arrays")
        groups.append(list(item))
    return groups


def main() -> None:
    if len(sys.argv) != 2:
        die("usage: hack-flags.py <crate-dir>")
    crate = Path(sys.argv[1])

    config: dict[str, object] = {}
    path = crate / "hack.toml"
    if path.is_file():
        with path.open("rb") as fh:
            config = tomllib.load(fh)

    unknown = set(config) - KNOWN
    if unknown:
        die(f"{path}: unknown key(s): {', '.join(sorted(unknown))}")

    available = declared_features(crate)
    flags: list[str] = ["--feature-powerset"]

    depth = config.get("depth", DEFAULT_DEPTH)
    if not isinstance(depth, int):
        die(f"{path}: `depth` must be an integer")
    flags += [SCALAR_KEYS["depth"], str(depth)]

    for key, flag in LIST_KEYS.items():
        if key not in config:
            continue
        for group in normalise(key, config[key]):
            missing = [f for f in group if f not in available]
            if missing:
                die(
                    f"{path}: `{key}` names feature(s) the crate does not "
                    f"declare: {', '.join(missing)}"
                )
            flags += [flag, ",".join(group)]

    for key, flag in FLAG_KEYS.items():
        if config.get(key):
            flags.append(flag)

    print(" ".join(flags))


if __name__ == "__main__":
    main()
