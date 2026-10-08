<script lang="ts">
    interface Props {
        id: string; label: string; value: number; min: number; max: number;
        step?: number; unit?: string; disabled?: boolean; onchange: (value: number) => void;
    }
    let { id, label, value, min, max, step = 1, unit = '', disabled = false, onchange }: Props = $props();
    let displayed = $state(0);
    $effect(() => {
        // Disabling a control cancels its optimistic draft, including a rejected
        // edit acknowledged with the same authoritative value as before.
        if (disabled) { displayed = value; return; }
        displayed = value;
    });
    function change(event: Event) {
        if (disabled) return;
        const input = event.currentTarget as HTMLInputElement;
        const next = input.valueAsNumber;
        if (Number.isFinite(next)) {
            displayed = Math.min(max, Math.max(min, next));
            // Reuse PR22's normalization when a clamp equals the current state.
            input.value = String(displayed);
            onchange(displayed);
        }
    }
</script>
<div class="brush-control">
    <label for={id}>{label}<span>{unit}</span></label>
    <div class="inputs">
        <input {id} type="range" {min} {max} {step} {disabled} value={displayed} oninput={change} />
        <input type="number" aria-label={`${label} value`} {min} {max} {step} {disabled} value={displayed} onchange={change} />
    </div>
</div>
<style>
    .brush-control { margin: 12px 0; }
    label { display: flex; justify-content: space-between; font-size: 12px; color: #ddd; margin-bottom: 7px; }
    label span { color: #999; }
    .inputs { display: flex; align-items: center; gap: 10px; }
    input[type='range'] { width: 100%; min-width: 0; accent-color: #8aabff; }
    input[type='number'] { width: 68px; padding: 4px; color: #eee; background: #15171c; border: 1px solid #4b4c51; border-radius: 4px; }
    input:focus-visible { outline: 2px solid #9abaff; outline-offset: 2px; }
    input:disabled { opacity: 0.45; cursor: default; }
</style>
