<script lang="ts">
    import SearchBar from "@/components/elements/SearchBar.svelte"
    import ArcReactor from "@/components/elements/ArcReactor.svelte"
    import HDivider from "@/components/elements/HDivider.svelte"
    import Stats from "@/components/elements/Stats.svelte"
    import Footer from "@/components/Footer.svelte"

    import { Notification, Space  } from '@svelteuidev/core'
    import { InfoCircled } from 'radix-icons-svelte'
    import { onMount, onDestroy } from 'svelte'

    onMount(async () => {
        document.body.classList.add('assist-page');
	});

    onDestroy(async () => {
        document.body.classList.remove('assist-page');
	});

    import { is_listening } from "@/stores"
    let is_listening__val: boolean;
    is_listening.subscribe(value => {
        is_listening__val = value;
    });
</script>

<HDivider />
{#if !is_listening__val}
<Notification title='Внимание!' icon={InfoCircled} color='cyan' withCloseButton={false}>
В данный момент ассистент не прослушивает команды.<br />
Пожалуйста, <a href="/settings">перейдите в настройки</a> и введите ключ Picovoice.
</Notification>
<!-- <SearchBar /> -->
{:else}
<!-- <SearchBar /> -->
<ArcReactor />
{/if}
<HDivider no_margin />
<Stats />
<Footer />


<!-- 
<Title order={1}>This is h1 title</Title>
<Title order={1} variant='gradient' gradient={{from: 'blue', to: 'red', deg: 45}}>This is h1 title with a twist</Title>

<Menu>
  <Button slot="control" variant="gradient" gradient={{ from: 'blue', to: 'teal', deg: 50 }} radius="md" size="md">Toggle Menu</Button>
    <Menu.Label>Application</Menu.Label>
    <Menu.Item icon={Gear}>Settings</Menu.Item>
    <Menu.Item icon={ChatBubble}>Messages</Menu.Item>
    <Menu.Item icon={Camera}>Gallery</Menu.Item>
    <Menu.Item icon={MagnifyingGlass}>
        <svelte:fragment slot='rightSection'>
            <Text size="xs" color="dimmed">⌘K</Text>
        </svelte:fragment>
        Search
    </Menu.Item>

    <Divider />

    <Menu.Label>Danger zone</Menu.Label>
    <Menu.Item icon={Width}>Transfer my data</Menu.Item>
    <Menu.Item color="red" icon={Trash}>Delete my account</Menu.Item>
</Menu>

<Checkbox bind:checked={checked} label="I agree to sell my privacy" />
{checked}
{#if checked}
YEP!
{/if} -->