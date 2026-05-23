import sveltePreprocess from "svelte-preprocess"

export default {
    preprocess: [
        sveltePreprocess({
            typescript: true,
            scss: { silenceDeprecations: ["legacy-js-api"] },
        }),
    ],
}
