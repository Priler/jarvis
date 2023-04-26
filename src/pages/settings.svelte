<script lang="ts">
  import { invoke } from "@tauri-apps/api/tauri"
  import { goto } from '@roxi/routify'

  import HDivider from "@/components/elements/HDivider.svelte"
  import Footer from "@/components/Footer.svelte"

  import { Notification, Button, Text, Tabs, Space, Alert, Input, InputWrapper, NativeSelect  } from '@svelteuidev/core';
  import { Check, Mix, Code, Gear, InfoCircled } from 'radix-icons-svelte';

  // vars
  let available_microphones = [];
  let settings_saved = false;

  // settings field values
  let assistant_voice_val = ""; // shared
  let selected_microphone = "";

  let api_key__picovoice = "";
  let api_key__openai = "";

  // shared values
  import { assistant_voice } from "@/stores"
  assistant_voice.subscribe(value => {
    assistant_voice_val = value;
  });

  (async () => {
    // preload some vars
    let _available_microphones: Array<Number> = await invoke("pv_get_audio_devices");
    Object.entries(_available_microphones).forEach(entry => {
      const [k, v] = entry;
      
      available_microphones.push({
        label: v,
        value: k
      });
    });

    available_microphones = available_microphones; // update component options

    // load values from db
    // assistant_voice.set(await invoke("db_read", {key: "assistant_voice"}));
    selected_microphone = await invoke("db_read", {key: "selected_microphone"});

    api_key__picovoice = await invoke("db_read", {key: "api_key__picovoice"});
    api_key__openai = await invoke("db_read", {key: "api_key__openai"});
  })().catch(err => {
      console.error(err);
  });

  // subscribe to listen state
  import { startListening, stopListening } from "@/functions";

  // save values to db
  async function save_settings(event) {
    await invoke("db_write", {key: "assistant_voice", val: assistant_voice_val});
    await invoke("db_write", {key: "selected_microphone", val: selected_microphone});

    await invoke("db_write", {key: "api_key__picovoice", val: api_key__picovoice});
    await invoke("db_write", {key: "api_key__openai", val: api_key__openai});

    // update shared
    assistant_voice.set(assistant_voice_val);

    settings_saved = true;

    // restart listening everytime
    stopListening(() => {
      startListening();
    });
  }
</script>

<Space h="xl" />

<Alert icon={InfoCircled}  title="Внимание!" color="cyan" variant="outline">
  Приложение находится в <strong>БЕТА</strong> режиме.<br />
  Часть функций может работать некорректно.<br />
  Сообщайте обо всех найденных багах в <a href="https://t.me/hhsharebot" target="_blank">наш телеграм бот</a>.
</Alert>

<Space h="xl" />

{#if settings_saved }
<Notification title='Настройки сохранены!' icon={Check} color='teal' on:close="{() => {settings_saved = false}}"></Notification>
{/if}

<Space h="xl" />

<Tabs color='cyan' position="left">
  <Tabs.Tab label='Общее' icon={Gear}>
    <NativeSelect data={[
      { label: 'Jarvis ремейк (от Хауди)', value: 'jarvis-remake' },
      { label: 'Jarvis OG (из фильмов)', value: 'jarvis-og' }
    ]}
    label="Голос ассистента"
    description="Не все команды работают со всеми звуковыми пакетами."
    variant="filled"
    bind:value={assistant_voice_val}
   />
  </Tabs.Tab>
  <Tabs.Tab label='Устройства' icon={Mix}>
    <NativeSelect data={available_microphones}
    label="Выберите микрофон"
    description="Его будет слушать ассистент."
    variant="filled"
    bind:value={selected_microphone}
   />
  </Tabs.Tab>
  <Tabs.Tab label='API ключи' icon={Code}>
    <InputWrapper label="Ключ Picovoice">
      <Text size='xs'>Введите сюда свой ключ Picovoice.<br />Он выдается бесплатно при регистрации в <a href='https://console.picovoice.ai/' target="_blank">Picovoice Console</a>.</Text>
      <Space h="sm" />
      <Input icon={Code} placeholder='Ключ Picovoice' variant='filled' autocomplete="off"  bind:value={api_key__picovoice}/>
    </InputWrapper>
    <Space h="xl" />
    <InputWrapper label="Ключ OpenAI">
      <Text size='xs'>Введите сюда свой ключ OpenAI, он требуется для работы ChatGPT.<br />Получить его можно <a href="https://chat.openai.com/auth/login" target="_blank">на официальном сайте OpenAI</a>.</Text>
      <Space h="sm" />
      <Input icon={Code} placeholder='Ключ OpenAI' variant='filled' autocomplete="off" bind:value={api_key__openai}/>
    </InputWrapper>
  </Tabs.Tab>
</Tabs>

<Space h="xl" />

<Button color="lime" radius="md" size="sm" uppercase ripple fullSize on:click={save_settings}>
  Сохранить
</Button>
<Space h="sm" />
<Button color="gray" radius="md" size="sm" uppercase fullSize on:click={() => {$goto('/')}}>
  Назад
</Button>

<HDivider />
<Footer />