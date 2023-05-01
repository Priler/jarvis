<script lang="ts">
    import HDivider from "@/components/elements/HDivider.svelte"
    import Footer from "@/components/Footer.svelte"

    import { Notification, Space  } from '@svelteuidev/core';
    import { InfoCircled } from 'radix-icons-svelte';

    import { tg_official_link } from "@/stores";
</script>

<Space h="xl" />

<Notification title='[404] Этот раздел еще находится в разработке!' icon={InfoCircled} color='blue' withCloseButton={false}>
    Тут будет список команд + полноценный редактор команд.<br>
    Следите за обновлениями в <a href="{tg_official_link}" target="_blank">нашем телеграм канале</a>!
</Notification>

<div style="text-align: center;margin-top: 25px;">
    <img src="/media/images/tenor.gif" alt="bruh" width="320px">
</div>

<HDivider />
<Footer />