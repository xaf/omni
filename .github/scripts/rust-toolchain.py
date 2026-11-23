#!/usr/bin/env python3
"""Print the checked-out compiler pin for CI actions with a toolchain input."""

import tomllib
from pathlib import Path

with Path("rust-toolchain.toml").open("rb") as toolchain_file:
    channel = tomllib.load(toolchain_file)["toolchain"]["channel"]
if not isinstance(channel, str) or not channel or any(c.isspace() for c in channel):
    raise SystemExit("rust-toolchain.toml must specify a nonempty toolchain.channel")
print(channel)
