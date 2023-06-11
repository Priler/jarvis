<script>
    let search_q = "";
</script>

<div id="search-form" class="search" class:active={search_q != ""}>
    <form action="#" method="GET">
        <input bind:value={search_q} type="text" name="q" placeholder="Введите команду или скажите &laquo;Джарвис&raquo; ..." autocomplete="off" minlength="3" maxlength="30">
        <button type="submit"></button>
        <small>Enter</small>
    </form>
</div>