<script lang="ts">
    export let variant: "primary" | "secondary" | "ghost" = "secondary"
    export let size: "md" | "sm" = "md"
    export let disabled: boolean = false
    export let type: "button" | "submit" | "reset" = "button"

    let klass = ""
    export { klass as class }
</script>

<button
    {type}
    {disabled}
    class="btn btn-{variant} btn-{size} {klass}"
    on:click
>
    <slot />
</button>

<style lang="scss">
.btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--r-md);
    font-family: var(--font);
    font-weight: 600;
    text-transform: uppercase;
    cursor: pointer;
    transition: var(--ease);
    white-space: nowrap;
    outline: none;

    &:focus-visible {
        outline: 2px solid var(--border-focus);
        outline-offset: 2px;
    }
}

.btn-md {
    height: 44px;
    padding: 0 20px;
    font-size: 0.7rem;
    letter-spacing: 0.14em;
}

.btn-sm {
    height: 30px;
    padding: 0 10px;
    font-size: 0.65rem;
    letter-spacing: 0.1em;
}

/* ── Primary — accent gradient ── */
.btn-primary {
    background: linear-gradient(180deg, var(--accent-glow-lg), rgba(0,180,255,0.08));
    border: 1px solid rgba(var(--accent-rgb), 0.25);
    color: var(--accent);
    box-shadow: inset 0 0 20px var(--accent-glow-sm), 0 0 20px rgba(var(--accent-rgb), 0.06);

    &:hover:not(:disabled) {
        background: linear-gradient(180deg, var(--accent-glow-xl), rgba(0,180,255,0.12));
        border-color: rgba(var(--accent-rgb), 0.4);
        box-shadow: inset 0 0 24px var(--accent-glow-md), 0 0 28px rgba(var(--accent-rgb), 0.14);
        transform: translateY(-1px);
    }

    &:active:not(:disabled) { transform: scale(0.99); }

    &:disabled {
        opacity: 0.3;
        cursor: not-allowed;
        box-shadow: none;
    }
}

/* ── Secondary — bordered, transparent ── */
.btn-secondary {
    background: transparent;
    border: 1px solid var(--border);
    color: rgba(var(--white-rgb), 0.5);

    &:hover:not(:disabled) {
        border-color: rgba(var(--white-rgb), 0.16);
        color: rgba(var(--white-rgb), 0.75);
        background: rgba(var(--white-rgb), 0.04);
    }

    &:disabled { opacity: 0.4; cursor: not-allowed; }
}

/* ── Ghost — minimal, dim ── */
.btn-ghost {
    background: transparent;
    border: 1px solid rgba(var(--white-rgb), 0.07);
    color: rgba(var(--white-rgb), 0.38);
    opacity: 0.55;

    &:hover:not(:disabled) {
        opacity: 1;
        border-color: rgba(var(--white-rgb), 0.08);
        color: rgba(var(--white-rgb), 0.55);
        background: rgba(var(--white-rgb), 0.025);
    }

    &:disabled { cursor: not-allowed; }
}
</style>
