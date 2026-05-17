import { defineConfig } from "vitest/config"
import { svelte } from "@sveltejs/vite-plugin-svelte"
import sveltePreprocess from "svelte-preprocess"
import { createLogger } from "vite"

// vite-plugin-svelte v3 (Svelte 4) sets optimizeDeps.esbuildOptions, which Vite 5.4
// deprecated in favour of rolldownOptions. The v3 branch is closed (v4 requires Svelte 5),
// so the fix cannot come from upstream — suppress the known warning here.
const logger = createLogger()
const _warn = logger.warn.bind(logger)
logger.warn = (msg, opts) => {
    if (msg.includes("optimizeDeps.esbuildOptions")) return
    _warn(msg, opts)
}

export default defineConfig({
    customLogger: logger,
    plugins: [
        svelte({
            hot: !process.env.VITEST,
            preprocess: [sveltePreprocess({ typescript: true, scss: { silenceDeprecations: ["legacy-js-api"] } })],
        }),
    ],
    resolve: {
        tsconfigPaths: true,
        // Svelte 4 exports "svelte" with a "browser" condition for the runtime
        // (onMount, tick, etc.) and a "default" condition for ssr.js where all
        // lifecycle hooks are no-ops. Without "browser" here, Vitest (Node.js)
        // resolves the SSR build and onMount never fires in component tests.
        conditions: ["browser"],
    },
    test: {
        globals: true,
        include: ["src/**/*.test.ts"],
        // Default environment for unit tests. Component tests override this
        // per-file with the `// @vitest-environment happy-dom` docblock annotation
        // (environmentMatchGlobs doesn't resolve Windows paths reliably).
        environment: "node",
        setupFiles: ["src/__tests__/setup.ts"],
    },
})
