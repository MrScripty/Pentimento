<script lang="ts">
    interface Props {
        id: string; label: string; value: number; min: number; max: number;
        step?: number; unit?: string; onchange: (value: number) => void;
    }
    let { id, label, value, min, max, step = 1, unit = '', onchange }: Props = $props();
    function change(event: Event) {
        const next = (event.currentTarget as HTMLInputElement).valueAsNumber;
        if (Number.isFinite(next)) onchange(Math.min(max, Math.max(min, next)));
    }
</script>
<div class="brush-control">
    <label for={id}>{label}<span>{unit}</span></label>
    <div class="inputs">
        <input {id} type="range" {min} {max} {step} {value} oninput={change} />
        <input type="number" aria-label={`${label} value`} {min} {max} {step} {value} onchange={change} />
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
</style>
