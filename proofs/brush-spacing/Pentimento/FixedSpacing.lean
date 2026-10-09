import Mathlib

/-!
Exact-real, fixed-positive-spacing phase accounting only. `a` is unconsumed
arc length; `L` is NEW segment length, never distance from the previous dab.
No claim is made about floating-point code, positions, or changing spacing.
-/
namespace Pentimento.FixedSpacing

noncomputable section

/-- Number of spacing crossings in one new segment. -/
def crossings (h a L : ℝ) : ℕ := ⌊(a + L) / h⌋₊

/-- Unconsumed arc length after emitting each crossing exactly once. -/
def residual (h a L : ℝ) : ℝ := a + L - (crossings h a L : ℝ) * h

/-- State is (number of emitted dabs, unconsumed arc length). -/
abbrev State := ℕ × ℝ

def advance (h L : ℝ) (s : State) : State :=
  (s.1 + crossings h s.2 L, residual h s.2 L)

/-- A nonempty stroke starts once with policy 0 (sculpt) or 1 (paint).
An empty stroke has no start event and emits zero dabs. -/
def start (initialDab : Fin 2) : State := (initialDab.val, 0)

theorem crossings_eq_of_bounds {h a L : ℝ} {k : ℕ}
    (hh : 0 < h) (hlo : (k : ℝ) * h ≤ a + L)
    (hhi : a + L < ((k : ℝ) + 1) * h) : crossings h a L = k := by
  apply (Nat.floor_eq_iff (show 0 ≤ (a + L) / h from
    div_nonneg (le_trans (mul_nonneg (Nat.cast_nonneg k) hh.le) hlo) hh.le)).2
  exact ⟨(le_div_iff₀ hh).2 hlo, (div_lt_iff₀ hh).2 hhi⟩

theorem residual_bounds {h a L : ℝ} (hh : 0 < h)
    (ha : 0 ≤ a) (hL : 0 ≤ L) :
    0 ≤ residual h a L ∧ residual h a L < h := by
  have hlo := (le_div_iff₀ hh).1
    (Nat.floor_le (div_nonneg (add_nonneg ha hL) hh.le))
  have hhi := (div_lt_iff₀ hh).1 (Nat.lt_floor_add_one ((a + L) / h))
  change 0 ≤ a + L - (⌊(a + L) / h⌋₊ : ℝ) * h ∧
    a + L - (⌊(a + L) / h⌋₊ : ℝ) * h < h
  constructor <;> nlinarith

theorem conservation (h L : ℝ) (s : State) :
    ((advance h L s).1 : ℝ) * h + (advance h L s).2 =
      (s.1 : ℝ) * h + s.2 + L := by
  simp only [advance, residual, Nat.cast_add]
  ring

theorem count_from_zero (h L : ℝ) :
    (advance h L (0, 0)).1 = ⌊L / h⌋₊ := by
  simp [advance, crossings]

theorem count_with_initial_policy (h L : ℝ) (b : Fin 2) :
    (advance h L (start b)).1 = b.val + ⌊L / h⌋₊ := by
  simp [advance, start, crossings]

theorem initial_policy_at_most_one (b : Fin 2) : b.val ≤ 1 := by
  omega

theorem crossings_composition {h a L₁ L₂ : ℝ} (hh : 0 < h)
    (ha : 0 ≤ a) (hL₁ : 0 ≤ L₁) (hL₂ : 0 ≤ L₂) :
    crossings h a (L₁ + L₂) =
      crossings h a L₁ + crossings h (residual h a L₁) L₂ := by
  have hr₁ := residual_bounds hh ha hL₁
  have hr₂ := residual_bounds hh hr₁.1 hL₂
  apply crossings_eq_of_bounds hh
  · push_cast
    dsimp [residual] at hr₂ ⊢
    nlinarith [hr₂.1]
  · push_cast
    dsimp [residual] at hr₂ ⊢
    nlinarith [hr₂.2]

theorem residual_composition {h a L₁ L₂ : ℝ} (hh : 0 < h)
    (ha : 0 ≤ a) (hL₁ : 0 ≤ L₁) (hL₂ : 0 ≤ L₂) :
    residual h (residual h a L₁) L₂ = residual h a (L₁ + L₂) := by
  have hq := crossings_composition hh ha hL₁ hL₂
  simp only [residual] at hq ⊢
  rw [hq]
  push_cast
  ring

theorem advance_composition {h L₁ L₂ : ℝ} (s : State) (hh : 0 < h)
    (ha : 0 ≤ s.2) (hL₁ : 0 ≤ L₁) (hL₂ : 0 ≤ L₂) :
    advance h L₂ (advance h L₁ s) = advance h (L₁ + L₂) s := by
  apply Prod.ext
  · simp only [advance, crossings_composition hh ha hL₁ hL₂, Nat.add_assoc]
  · exact residual_composition hh ha hL₁ hL₂

theorem crossings_zero {h a : ℝ} (hh : 0 < h) (ha : 0 ≤ a)
    (hah : a < h) : crossings h a 0 = 0 := by
  apply crossings_eq_of_bounds hh
  · simpa using ha
  · simpa using hah

theorem advance_zero {h : ℝ} (s : State) (hh : 0 < h)
    (ha : 0 ≤ s.2) (hah : s.2 < h) : advance h 0 s = s := by
  have hq := crossings_zero hh ha hah
  apply Prod.ext <;> simp [advance, residual, hq]

/-- Reaching exactly k spacings emits k crossings and leaves no residual. -/
theorem exact_boundary {h a L : ℝ} (hh : 0 < h) (k : ℕ)
    (heq : a + L = (k : ℝ) * h) :
    crossings h a L = k ∧ residual h a L = 0 := by
  have hq : crossings h a L = k := by
    apply crossings_eq_of_bounds hh
    · exact heq.ge
    · rw [heq]
      nlinarith
  exact ⟨hq, by simp [residual, hq, heq]⟩

/-- A boundary already consumed cannot be emitted again by a zero segment. -/
theorem boundary_no_double_emission {h a L : ℝ} (hh : 0 < h) (k : ℕ)
    (heq : a + L = (k : ℝ) * h) :
    crossings h (residual h a L) 0 = 0 := by
  rw [(exact_boundary hh k heq).2]
  exact crossings_zero hh (le_refl 0) hh

end
end Pentimento.FixedSpacing
