import Pentimento
import Lean.Util.CollectAxioms

/-! Fail closed on the transitive kernel dependencies of every public
Pentimento theorem and the seven source definitions, including rational fixtures.
Lean's collectAxioms reads env.checked, not unchecked elaborator promises.
Only the standard foundations below are allowed: no sorryAx, custom
assumptions, or Lean.ofReduceBool/native-evaluation axioms. -/
open Lean Elab Command

run_cmd do
  let env ← getEnv
  let allowed : Array Name := #[`propext, `Classical.choice, `Quot.sound]
  -- Compiler-generated runtime specializations are not proof declarations.
  -- Audit source definitions explicitly; never whitelist their placeholder axioms.
  let sourceDefinitions : Array Name := #[
    `Pentimento.FixedSpacing.crossings, `Pentimento.FixedSpacing.residual,
    `Pentimento.FixedSpacing.State, `Pentimento.FixedSpacing.advance,
    `Pentimento.FixedSpacing.start, `Pentimento.Fixtures.step, `Pentimento.Fixtures.run]
  let expectedTheorems : Array Name := #[
    `Pentimento.FixedSpacing.crossings_eq_of_bounds,
    `Pentimento.FixedSpacing.residual_bounds,
    `Pentimento.FixedSpacing.conservation,
    `Pentimento.FixedSpacing.count_from_zero,
    `Pentimento.FixedSpacing.count_with_initial_policy,
    `Pentimento.FixedSpacing.initial_policy_at_most_one,
    `Pentimento.FixedSpacing.crossings_composition,
    `Pentimento.FixedSpacing.residual_composition,
    `Pentimento.FixedSpacing.advance_composition,
    `Pentimento.FixedSpacing.crossings_zero,
    `Pentimento.FixedSpacing.advance_zero,
    `Pentimento.FixedSpacing.exact_boundary,
    `Pentimento.FixedSpacing.boundary_no_double_emission,
    `Pentimento.Fixtures.sculpt_correct_phase,
    `Pentimento.Fixtures.sculpt_no_early_crossing,
    `Pentimento.Fixtures.paint_preserves_carry,
    `Pentimento.Fixtures.rational_split,
    `Pentimento.Fixtures.rational_unsplit,
    `Pentimento.Fixtures.multiple_crossings,
    `Pentimento.Fixtures.zero_segment,
    `Pentimento.Fixtures.exact_boundary_once,
    `Pentimento.Fixtures.no_forced_endpoint,
    `Pentimento.Fixtures.empty_stroke]
  for name in sourceDefinitions ++ expectedTheorems do
    let _ ← getConstInfo name
    pure ()
  let mut audited := 0
  for (name, info) in env.constants.toList do
    let isProofOrAxiom := match info with
      | .thmInfo _ => true
      | .axiomInfo _ => true
      | _ => false
    if name.toString.startsWith "Pentimento." &&
        (isProofOrAxiom || sourceDefinitions.contains name) then
      let axioms ← Lean.collectAxioms name
      for axiomName in axioms do
        unless allowed.contains axiomName do
          throwError "Disallowed axiom {axiomName} in {name}"
      logInfo m!"AUDITED {name}: {axioms}"
      audited := audited + 1
  if audited == 0 then
    throwError "No Pentimento declarations were audited"
  logInfo m!"Axiom audit passed for {audited} declarations"
