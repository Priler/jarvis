// ### STYLES
import "./css/tokens.css"
import "./css/main.scss"
import "./css/styles.scss"
import "./css/settings.scss"
import "./css/system.scss"
import "./css/arc-reactor.scss"

// ### APP
import App from "./App.svelte"

const app = new App({
    target: document.getElementById("app")!
})

export default app
