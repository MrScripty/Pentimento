import Pentimento
import Lean.Util.CollectAxioms

/-! Fail closed on the transitive kernel dependencies of EVERY public
Pentimento declaration, including definitions and rational fixtures.
Lean's collectAxioms reads env.checked, not unchecked elaborator promises.
Only the standard foundations below are allowed: no sorryAx, custom
assumptions, or Lean.ofReduceBool/native-evaluation axioms. -/
open Lean Elab Command

run_cmd do
  let env ← getEnv
  let allowed : Array Name := #[`propext, `Classical.choice, `Quot.sound]
  let mut audited := 0
  for (name, _) in env.constants.toList do
    if name.toString.startsWith "Pentimento." then
      let axioms ← Lean.collectAxioms name
      for axiomName in axioms do
        unless allowed.contains axiomName do
          throwError "Disallowed axiom {axiomName} in {name}"
      logInfo m!"AUDITED {name}: {axioms}"
      audited := audited + 1
  if audited == 0 then
    throwError "No Pentimento declarations were audited"
  logInfo m!"Axiom audit passed for {audited} declarations"
