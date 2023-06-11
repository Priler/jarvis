<script>
    export let no_margin = false;
</script>

<div class="h-divider" class:no-margin="{no_margin}"></div>

<style lang="scss">
    .h-divider {
        margin: 20px 0;
        height: 40px;
        background-image: url(media/images/decor.png);
        background-position: center;

        &.no-margin {
            margin: 0;
        }
    }
</style>