<script lang="ts">
    import { goto, isActive } from "@roxi/routify"
    import { tStore } from "@/stores"
    import WindowFrame from "@/components/layout/WindowFrame.svelte"

    $: t = $tStore
</script>

<header class="header">

    <!-- Level 1: System Shell -->
    <div class="shell-bar">
        <a class="logo" href="/" title="JARVIS" on:click|preventDefault={() => $goto('/')}>
            <svg class="logo-icon" width="36" height="36" viewBox="0 0 40 40" fill="none" xmlns="http://www.w3.org/2000/svg">
                <circle cx="20" cy="20" r="18" stroke="currentColor" stroke-width="0.75" opacity="0.28"/>
                <g class="logo-spin">
                    <circle cx="20" cy="20" r="13" stroke="currentColor" stroke-width="1"
                            stroke-dasharray="3.5 4.5" opacity="0.58"/>
                </g>
                <circle cx="20" cy="20" r="8" stroke="currentColor" stroke-width="1.25" opacity="0.78"/>
                <circle cx="20" cy="20" r="3.5" fill="currentColor"/>
                <line x1="20" y1="2" x2="20" y2="7" stroke="currentColor" stroke-width="1" opacity="0.40"/>
                <line x1="20" y1="33" x2="20" y2="38" stroke="currentColor" stroke-width="1" opacity="0.40"/>
                <line x1="2" y1="20" x2="7" y2="20" stroke="currentColor" stroke-width="1" opacity="0.40"/>
                <line x1="33" y1="20" x2="38" y2="20" stroke="currentColor" stroke-width="1" opacity="0.40"/>
            </svg>
            <span class="brand-name">JARVIS</span>
        </a>
        <WindowFrame />
    </div>

    <!-- Level 2: Application Navigation -->
    <nav class="nav-bar" aria-label={t('nav-main') || 'Main navigation'}>
        <a
            class="nav-tab"
            href="/"
            class:active={$isActive('/', {}, { recursive: false })}
            aria-current={$isActive('/', {}, { recursive: false }) ? 'page' : undefined}
            on:click|preventDefault={() => $goto('/')}
        >
            {t('header-home')}
        </a>
        <a
            class="nav-tab"
            href="/commands"
            class:active={$isActive('/commands')}
            aria-current={$isActive('/commands') ? 'page' : undefined}
            on:click|preventDefault={() => $goto('/commands')}
        >
            {t('header-commands')}
        </a>
        <a
            class="nav-tab"
            href="/settings"
            class:active={$isActive('/settings')}
            aria-current={$isActive('/settings') ? 'page' : undefined}
            on:click|preventDefault={() => $goto('/settings')}
        >
            {t('header-settings')}
        </a>
        <a
            class="nav-tab"
            href="/system"
            class:active={$isActive('/system')}
            aria-current={$isActive('/system') ? 'page' : undefined}
            on:click|preventDefault={() => $goto('/system')}
        >
            {t('header-system')}
        </a>
        <div class="scan-line" aria-hidden="true"></div>
    </nav>

</header>

<style lang="scss">
.header {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    position: relative;
    z-index: 100;
}

/* ── Level 1: System shell ── */
.shell-bar {
    height: var(--shell-h);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 0 0 20px;
    background: linear-gradient(180deg, rgba(11,15,22,0.98) 0%, rgba(7,10,15,0.96) 100%);
    box-shadow: inset 0 1px 0 rgba(0,229,255,0.08),
                inset 0 -10px 18px rgba(0,0,0,0.16);
    -webkit-app-region: drag;
    position: relative;
    flex-shrink: 0;

    &::after {
        content: '';
        position: absolute;
        bottom: 0;
        left: 0;
        right: 0;
        height: 1px;
        background: var(--shell-separator);
        pointer-events: none;
    }
}

/* ── Logo / brand ── */
.logo {
    display: flex;
    align-items: center;
    gap: 9px;
    text-decoration: none;
    color: var(--accent);
    -webkit-app-region: no-drag;
    position: relative;
    transition: opacity 140ms ease;

    &::before {
        content: '';
        position: absolute;
        left: -4px;
        top: 50%;
        transform: translateY(-50%);
        width: 46px;
        height: 46px;
        background: radial-gradient(circle, rgba(0,229,255,0.10), transparent 70%);
        border-radius: 50%;
        pointer-events: none;
    }

    &:hover { opacity: 0.75; }
}

.logo-icon {
    filter: drop-shadow(0 0 8px rgba(0,229,255,0.12));
    flex-shrink: 0;
}

.logo-spin {
    transform-origin: 20px 20px;
    animation: logo-spin 24s linear infinite;
}

@keyframes logo-spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
}

.brand-name {
    font-size: 1.2rem;
    font-weight: 600;
    letter-spacing: 0.10em;
    color: var(--text);
    text-transform: uppercase;
    line-height: 1;
    padding-right: 0.05em;
}

/* ── Level 2: Application navigation ── */
.nav-bar {
    height: var(--nav-h);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 28px;
    background: linear-gradient(180deg, rgba(7,10,15,0.92) 0%, rgba(5,8,12,0.90) 100%);
    position: relative;
    flex-shrink: 0;

    &::after {
        content: '';
        position: absolute;
        bottom: 0;
        left: 0;
        right: 0;
        height: 1px;
        background: var(--shell-separator);
        pointer-events: none;
    }
}

/* ── Nav tabs ── */
.nav-tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    height: 100%;
    padding: 0;
    background: transparent;
    border: none;
    color: rgba(255,255,255,0.55);
    font-family: var(--font);
    font-size: 13px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    cursor: pointer;
    text-decoration: none;
    transition: color 140ms ease;
    white-space: nowrap;

    &::after {
        content: '';
        position: absolute;
        bottom: 0;
        left: 0;
        right: 0;
        height: 2px;
        background: var(--accent);
        box-shadow: 0 0 8px var(--accent-glow-lg);
        opacity: 0;
        transition: opacity 140ms ease;
    }

    &:hover { color: rgba(255,255,255,0.88); }

    &:focus-visible {
        outline: 2px solid rgba(0,229,255,0.55);
        outline-offset: -2px;
        border-radius: 2px;
    }

    &.active {
        color: var(--text);
        font-weight: 700;

        &::after { opacity: 1; }
    }
}

/* ── Scan line ── */
.scan-line {
    position: absolute;
    bottom: 0;
    left: -30%;
    width: 30%;
    height: 1px;
    background: linear-gradient(90deg, transparent, rgba(0,229,255,0.28), transparent);
    animation: scan-sweep 8s linear infinite;
    pointer-events: none;
}

@keyframes scan-sweep {
    from { left: -30%; }
    to   { left: 110%; }
}

@media (prefers-reduced-motion: reduce) {
    .logo-spin { animation: none; }
    .scan-line { display: none; }
    .nav-tab   { transition: none; }
}
</style>
