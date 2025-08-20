<script lang="ts">
  // IMPORTS
  import { invoke } from "@tauri-apps/api/tauri"
  import { goto } from '@roxi/routify'
  import { onMount } from 'svelte'
  import { startListening, stopListening, showInExplorer } from "@/functions";
  import { setTimeout } from 'worker-timers';

  import { feedback_link, log_file_path } from "@/stores";

  // COMPONENTS & STUFF
  import HDivider from "@/components/elements/HDivider.svelte"
  import Footer from "@/components/Footer.svelte"

  import { Notification, Button, Text, Tabs, Space, Alert, Input, InputWrapper, NativeSelect  } from '@svelteuidev/core';
  import { Check, Mix, Cube, Code, Gear, QuestionMarkCircled, CrossCircled } from 'radix-icons-svelte';

  // VARIABLES

  let available_microphones = [];
  let settings_saved = false;
  let save_button_disabled = false;

  let assistant_voice_val = ""; // shared
  let selected_microphone = "";

  let selected_wake_word_engine = "";
  let api_key__picovoice = "";
  let api_key__openai = "";

  // SHARED VALUES
  import { assistant_voice } from "@/stores"
  assistant_voice.subscribe(value => {
    assistant_voice_val = value;
  });

  // FUNCTIONS
  async function save_settings() {
    save_button_disabled = true; // disable save button for a while
    settings_saved = false; // hide alert

    await invoke("db_write", {key: "assistant_voice", val: assistant_voice_val});
    await invoke("db_write", {key: "selected_microphone", val: selected_microphone});

    await invoke("db_write", {key: "selected_wake_word_engine", val: selected_wake_word_engine});
    await invoke("db_write", {key: "api_key__picovoice", val: api_key__picovoice});
    await invoke("db_write", {key: "api_key__openai", val: api_key__openai});

    // update shared
    assistant_voice.set(assistant_voice_val);

    settings_saved = true; // show alert
    setTimeout(() => {
      settings_saved = false; // hide alert again after N seconds
    }, 5000);

    setTimeout(() => {
      save_button_disabled = false; // enable save button again
    }, 1000);

    // restart listening everytime new settings is saved
    stopListening(() => {
      startListening();
    });
  }

  // CODE
  onMount(async () => {
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

    selected_wake_word_engine = await invoke("db_read", {key: "selected_wake_word_engine"});
    api_key__picovoice = await invoke("db_read", {key: "api_key__picovoice"});
    api_key__openai = await invoke("db_read", {key: "api_key__openai"});
	});

</script>

<Space h="xl" />

<Notification title='БЕТА версия!' icon={QuestionMarkCircled} color='blue' withCloseButton={false}>
  Часть функций может работать некорректно.<br />
  Сообщайте обо всех найденных багах в <a href="{feedback_link}" target="_blank">наш телеграм бот</a>.
  <Space h="sm" />
  <Button color="gray" radius="md" size="xs" uppercase on:click={() => {showInExplorer(log_file_path)}}>Открыть папку с логами</Button>
</Notification>

<Space h="xl" />

{#if settings_saved }
<Notification title='Настройки сохранены!' icon={Check} color='teal' on:close="{() => {settings_saved = false}}"></Notification>
<Space h="xl" />
{/if}

<Tabs class="form" color='#8AC832' position="left">
  <Tabs.Tab label='Общее' icon={Gear}>
    <Space h="sm" />

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
    <Space h="sm" />

    <NativeSelect data={available_microphones}
    label="Выберите микрофон"
    description="Его будет слушать ассистент."
    variant="filled"
    bind:value={selected_microphone}
   />
  </Tabs.Tab>
  <Tabs.Tab label='Нейросети' icon={Cube}>
    <Space h="sm" />

    <NativeSelect data={[
      { label: 'Rustpotter', value: 'rustpotter' },
      { label: 'Vosk (медленный)', value: 'vosk' },
      { label: 'Picovoice Porcupine (требует API ключ)', value: 'picovoice' }
    ]}
    label="Распознавание активационной фразы (Wake Word)"
    description="Выберите, какая нейросеть будет отвечать за распознавание активационной фразы."
    variant="filled"
    bind:value={selected_wake_word_engine}
    />

    {#if selected_wake_word_engine == "picovoice"}
      <Space h="sm" />
      <Alert title="Внимание!" color="#868E96" variant="outline">

          <Notification title='Эта нейросеть работает не у всех!' icon={CrossCircled} color='orange' withCloseButton={false}>
            Мы ждем официального патча от разработчиков.
          </Notification>
          <Space h="sm" />

          <Text size='sm' color="gray">
            Введите сюда свой ключ Picovoice.<br />
            Он выдается бесплатно при регистрации в <a href='https://console.picovoice.ai/' target="_blank">Picovoice Console</a>.<br>
          </Text>
          <Space h="sm" />
          <Input icon={Code} placeholder='Ключ Picovoice' variant='filled' autocomplete="off"  bind:value={api_key__picovoice}/>

      </Alert>
    {/if}

    <Space h="xl" />

    <InputWrapper label="Ключ OpenAI">
      <!-- <Text size='sm' color="gray">Введите сюда свой ключ OpenAI, он требуется для работы ChatGPT.<br />Получить его можно <a href="https://chat.openai.com/auth/login" target="_blank">на официальном сайте OpenAI</a>.</Text> -->
      <Text size='sm' color="gray">В данный момент ChatGPT <u>не поддерживается</u>. Он будет добавлен в ближайших обновлениях.</Text>
      <Space h="sm" />
      <Input icon={Code} placeholder='Ключ OpenAI' variant='filled' autocomplete="off" bind:value={api_key__openai} disabled/>
    </InputWrapper>
  </Tabs.Tab>
</Tabs>

<Space h="xl" />

<Button color="lime" radius="md" size="sm" uppercase ripple fullSize on:click={save_settings} disabled={save_button_disabled}>
  Сохранить
</Button>
<Space h="sm" />
<Button color="gray" radius="md" size="sm" uppercase fullSize on:click={() => {$goto('/')}}>
  Назад
</Button>

<HDivider />
<Footer />