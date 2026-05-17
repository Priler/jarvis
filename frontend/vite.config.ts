import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import sveltePreprocess from "svelte-preprocess";
import tsconfigPaths from 'vite-tsconfig-paths'
import routify from '@roxi/routify/vite-plugin'

export default defineConfig({
  plugins: [
    svelte({
      preprocess: [
        sveltePreprocess({
          typescript: true,
          scss: { silenceDeprecations: ["legacy-js-api"] },
        }),
      ],
      onwarn: (warning, handler) => {
        handler(warning);
      },
    }),
    routify(),
    tsconfigPaths()
  ],

  css: {
    preprocessorOptions: {
      scss: {
        silenceDeprecations: ["legacy-js-api", "global-builtin", "color-functions", "import"],
      },
    },
  },

  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: process.env.TAURI_PLATFORM == "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});