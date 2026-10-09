"""Cheap negative controls: unsupported declarations cannot evade inventory."""
from pathlib import Path
import shutil
import tempfile
from check_sources import ROOT, check_sources

attacks = {
    "indented definition": ("Pentimento/Fixtures.lean", "\n  def unreviewed : Nat := 0\n"),
    "root definition": ("Pentimento.lean", "\ndef unreviewed : Nat := 0\n"),
    "attributed definition": ("Pentimento/Fixtures.lean", "\n@[simp] def unreviewed : Nat := 0\n"),
    "noncomputable definition": ("Pentimento/Fixtures.lean", "\nnoncomputable def unreviewed : Nat := 0\n"),
    "opaque declaration": ("Pentimento/Fixtures.lean", "\nopaque unreviewed : Nat := 0\n"),
}
for label, (relative, addition) in attacks.items():
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / "proofs"
        shutil.copytree(ROOT, root, ignore=shutil.ignore_patterns(".lake", "__pycache__"))
        path = root / relative
        path.write_text(path.read_text() + addition)
        try:
            check_sources(root)
        except AssertionError:
            print(f"PASS: source inventory rejected {label}")
        else:
            raise SystemExit(f"Source inventory accepted {label}")
