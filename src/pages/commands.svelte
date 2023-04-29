<script lang="ts">
    import HDivider from "@/components/elements/HDivider.svelte"
    import Footer from "@/components/Footer.svelte"

    import { Notification  } from '@svelteuidev/core';
    import { InfoCircled } from 'radix-icons-svelte';
</script>

<HDivider />
<Notification title='404' icon={InfoCircled} color='blue' withCloseButton={false}>
    Этот раздел еще находится в разработке.
</Notification>
<HDivider />
<Footer />