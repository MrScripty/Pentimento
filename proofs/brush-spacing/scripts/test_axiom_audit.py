"""Hosted test: deliberate invalid proof assumptions must fail the audit."""
from pathlib import Path
import subprocess
import tempfile

# Reuse precisely the production audit command, replacing only its imports.
audit = Path("AxiomAudit.lean").read_text()
command = audit[audit.index("open Lean Elab Command"):]
attacks = {
    "custom": "axiom Pentimento.customAssumption : False\n",
    "unfinished": "theorem Pentimento.unfinished : False := by sorry\n",
    "native": "theorem Pentimento.nativeEvaluation : 2 + 2 = (4 : Nat) := by native_decide\n",
}
for label, declaration in attacks.items():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".lean", dir=".") as f:
        f.write("import Pentimento\nimport Mathlib\nimport Lean.Util.CollectAxioms\n" + declaration + command)
        f.flush()
        result = subprocess.run(["lake", "env", "lean", f.name], text=True, capture_output=True)
    output = result.stdout + result.stderr
    if result.returncode == 0 or "Disallowed axiom" not in output:
        raise SystemExit(f"Audit negative control {label} did not fail correctly:\n{output}")
    print(f"PASS: audit rejected {label}")
