<!-- src/App.svelte -->
<script>
  import { Router } from "@roxi/routify";
  import { routes } from "../.routify/routes";

  import { SvelteUIProvider } from '@svelteuidev/core';

  import Events from "./Events.svelte";

  /** START LISTENING **/
  import { startListening } from "./functions";
  startListening();
</script>

<SvelteUIProvider themeObserver='dark' withNormalizeCSS withGlobalStyles>
  <Router {routes} />
</SvelteUIProvider>

<Events />