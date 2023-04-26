// Klondike project old CSS file
import "./css/main.scss";

// App current CSS file
import "./css/styles.scss";

// deploy app
import App from "./App.svelte";
const app = new App({
  target: document.getElementById("app"),
});

export default app;