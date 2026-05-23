<script context="module" lang="ts">
    export const load = ({ route }: { route: { url: string } }) => ({
        status: 404,
        error: '[Routify] Page could not be found.',
        props: { url: route.url },
    })
</script>

<script lang="ts">
    export let url: string
</script>

<div class="not-found">
    <p>404 — {url}</p>
</div>

<style>
.not-found {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-muted);
    font-size: 0.8rem;
}
</style>
