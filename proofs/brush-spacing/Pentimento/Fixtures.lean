import Mathlib

/-! Exact-rational executable examples of the same recurrence.
`norm_num` produces kernel-checked proof terms; native_decide is not used.
These examples do not establish refinement of the Rust implementations. -/
namespace Pentimento.Fixtures

def step (h L : ℚ) (s : ℕ × ℚ) : ℕ × ℚ :=
  let q : ℕ := ⌊(s.2 + L) / h⌋₊
  (s.1 + q, s.2 + L - (q : ℚ) * h)

def run (h : ℚ) (segments : List ℚ) (s : ℕ × ℚ) : ℕ × ℚ :=
  segments.foldl (fun state L => step h L state) s

-- h=10, new lengths [3,3,4]. Sculpt's old distance-from-last-dab
-- accumulation uses [3,6,10] instead, reaching a crossing too early.
theorem sculpt_correct_phase : run 10 [3, 3, 4] (0, 0) = (1, 0) := by norm_num [run, step, ← Int.floor_toNat]

theorem sculpt_no_early_crossing : run 10 [3, 3, 3] (0, 0) = (0, 9) := by norm_num [run, step, ← Int.floor_toNat]

-- Paint starts with one dab; [3,3,4] must preserve short-segment carry.
-- The old no-dab assignment a := L instead of a := a+L leaves a=4, n=1.
theorem paint_preserves_carry : run 10 [3, 3, 4] (1, 0) = (2, 0) := by norm_num [run, step, ← Int.floor_toNat]

theorem rational_split : run (5 / 2) [1 / 2, 3 / 4, 7 / 4] (0, 0) = (1, 1 / 2) := by norm_num [run, step, ← Int.floor_toNat]

theorem rational_unsplit : run (5 / 2) [3] (0, 0) = (1, 1 / 2) := by norm_num [run, step, ← Int.floor_toNat]

theorem multiple_crossings : step 10 37 (0, 4) = (4, 1) := by
  norm_num [step, ← Int.floor_toNat]
  norm_num [Int.toNat]

theorem zero_segment : step 10 0 (7, 4) = (7, 4) := by norm_num [run, step, ← Int.floor_toNat]

theorem exact_boundary_once : run 10 [10, 0, 0] (0, 0) = (1, 0) := by norm_num [run, step, ← Int.floor_toNat]

theorem no_forced_endpoint : step 10 9 (0, 0) = (0, 9) := by norm_num [run, step, ← Int.floor_toNat]

theorem empty_stroke : run 10 [] (0, 0) = (0, 0) := by norm_num [run, step, ← Int.floor_toNat]

end Pentimento.Fixtures
