"""Cheap, deliberately narrow source grammar; NOT a Lean kernel check.
New Lean syntax/declaration forms require an explicit review of this gate.
"""
from pathlib import Path
import json
import hashlib
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]

# Exact reviewed proof source: additions cannot bypass declaration parsing.
# Change these only with a source-and-audit review, never regenerate in CI.
REVIEWED_SOURCE_SHA256 = {'Pentimento.lean': '5f0f8a271d953b6f22c45efe37b6c72a4cf764f972e7af46f4d6bed8a75a98fc', 'Pentimento/FixedSpacing.lean': '1db5bf9a425c0191dc1915438b140645bb78d0775eb19ae570b6b15cd7186c61', 'Pentimento/Fixtures.lean': '1da0bbfd1af88fcf4ec54e5dec63440c775149e0296fc3a3c9559cebdf844233'}


def strip_comments(source):
    """Handle nested Lean block comments, rejecting unterminated comments."""
    output, index, depth = [], 0, 0
    while index < len(source):
        if source.startswith("/-", index):
            depth += 1
            index += 2
        elif depth and source.startswith("-/", index):
            depth -= 1
            index += 2
        elif depth:
            if source[index] == "\n":
                output.append("\n")
            index += 1
        elif source.startswith("--", index):
            end = source.find("\n", index)
            index = len(source) if end < 0 else end
        else:
            output.append(source[index])
            index += 1
    assert depth == 0, "unterminated block comment"
    return "".join(output)


def check_sources(root=ROOT):
    assert (root / "lean-toolchain").read_text().strip() == "leanprover/lean4:v4.19.0"
    config = tomllib.loads((root / "lakefile.toml").read_text())
    rev = "c44e0c8ee63ca166450922a373c7409c5d26b00b"
    assert config["require"] == [{"name": "mathlib", "git": "https://github.com/leanprover-community/mathlib4.git", "rev": rev}]
    manifest = json.loads((root / "lake-manifest.json").read_text())
    assert next(p for p in manifest["packages"] if p["name"] == "mathlib")["rev"] == rev
    assert all(re.fullmatch(r"[a-f0-9]{40}", p["rev"]) for p in manifest["packages"])
    modules = {"FixedSpacing": "Pentimento.FixedSpacing", "Fixtures": "Pentimento.Fixtures"}
    expected_files = {"Pentimento.lean", "AxiomAudit.lean"} | {f"Pentimento/{m}.lean" for m in modules}
    actual_files = {str(p.relative_to(root)) for p in root.rglob("*.lean") if ".lake" not in p.relative_to(root).parts}
    assert actual_files == expected_files, "unreviewed Lean file inventory"
    assert strip_comments((root / "Pentimento.lean").read_text()).split() == [
        "import", "Pentimento.FixedSpacing", "import", "Pentimento.Fixtures"
    ], "root module must contain only the reviewed imports"
    for relative, expected in REVIEWED_SOURCE_SHA256.items():
        assert hashlib.sha256((root / relative).read_bytes()).hexdigest() == expected, "proof source changed: review inventory and update explicit digest"
    actual_declarations = set()
    audit = (root / "AxiomAudit.lean").read_text()
    expected_declarations = set(re.findall(r"`(Pentimento\.[A-Za-z0-9_.]+)", audit))
    forbidden = r"\b(sorry|admit|axiom|native_decide|unsafe|opaque|lemma|structure|class|instance|inductive|coinductive|mutual|example|syntax|macro|elab|initialize|builtin_initialize|attribute|export|alias|include|omit|open|variable|parameter|universe|notation|local|scoped|private|protected|set_option)\b"
    for module, namespace in modules.items():
        path = root / "Pentimento" / f"{module}.lean"
        source = strip_comments(path.read_text())
        assert '"' not in source and "@[" not in source and "#" not in source, "unreviewed source syntax"
        assert not re.search(forbidden, source), (path, "unsupported declaration or command")
        assert re.findall(r"\bimport\s+(\S+)", source) == ["Mathlib"], path
        assert re.findall(r"\bnamespace\s+(\S+)", source) == [namespace], path
        for line in source.splitlines():
            if re.search(r"\bnoncomputable\b", line):
                assert line == "noncomputable section", "unsupported noncomputable declaration"
            if re.search(r"\b(def|abbrev|theorem)\b", line):
                declaration = re.match(r"^(def|abbrev|theorem) ([A-Za-z][A-Za-z0-9_]*)\b", line)
                assert declaration, "only unmodified, column-zero declarations are reviewed"
                assert len(re.findall(r"\b(def|abbrev|theorem)\b", line)) == 1, line
                actual_declarations.add(namespace + "." + declaration.group(2))
    assert actual_declarations == expected_declarations, actual_declarations ^ expected_declarations


if __name__ == "__main__":
    check_sources()
    print("PASS: pins, source grammar, exact file/declaration inventory (not compilation)")
