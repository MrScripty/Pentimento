"""Cheap source checks. This is NOT a Lean elaboration/kernel check."""
from pathlib import Path
import json
import re
import tomllib

root = Path(__file__).resolve().parents[1]
assert (root / "lean-toolchain").read_text().strip() == "leanprover/lean4:v4.19.0"
config = tomllib.loads((root / "lakefile.toml").read_text())
rev = "c44e0c8ee63ca166450922a373c7409c5d26b00b"
assert config["require"] == [{"name": "mathlib", "git": "https://github.com/leanprover-community/mathlib4.git", "rev": rev}]
manifest = json.loads((root / "lake-manifest.json").read_text())
assert next(p for p in manifest["packages"] if p["name"] == "mathlib")["rev"] == rev
assert all(re.fullmatch(r"[a-f0-9]{40}", p["rev"]) for p in manifest["packages"])
for path in root.glob("Pentimento/**/*.lean"):
    source = path.read_text()
    source = re.sub(r"/-.*?-/", "", source, flags=re.S)
    source = re.sub(r"--[^\n]*", "", source)
    assert not re.search(r"\b(sorry|admit|axiom|native_decide|unsafe)\b", source), path
    assert "set_option debug.skipKernelTC" not in source, path
print("PASS: official pins, immutable manifest revisions, and source policy (not compilation)")
