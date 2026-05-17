import { defineConfig } from "vitest/config"
import { svelte } from "@sveltejs/vite-plugin-svelte"
import sveltePreprocess from "svelte-preprocess"

export default defineConfig({
    plugins: [
        svelte({
            hot: !process.env.VITEST,
            preprocess: [sveltePreprocess({ typescript: true })],
        }),
    ],
    resolve: {
        tsconfigPaths: true,
    },
    test: {
        globals: true,
        include: ["src/**/*.test.ts"],
        // Unit tests run in node; component tests run in happy-dom
        environmentMatchGlobs: [
            ["**/*.component.test.ts", "happy-dom"],
        ],
        environment: "node",
        setupFiles: ["src/__tests__/setup.ts"],
    },
})
