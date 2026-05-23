<script lang="ts">
    import Header from "@/components/Header.svelte"
</script>

<div id="wrapper">
    <Header />
    <main>
        <slot></slot>
    </main>
</div>
