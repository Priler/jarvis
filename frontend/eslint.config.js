import js from "@eslint/js"
import tsPlugin from "typescript-eslint"
import sveltePlugin from "eslint-plugin-svelte"

export default tsPlugin.config(
    js.configs.recommended,
    ...tsPlugin.configs.recommended,
    ...sveltePlugin.configs["flat/recommended"],
    {
        files: ["**/*.svelte"],
        languageOptions: {
            parserOptions: {
                parser: tsPlugin.parser,
            },
        },
    },
    {
        rules: {
            "no-console": "warn",
            "@typescript-eslint/no-explicit-any": "warn",
            "@typescript-eslint/no-unused-vars": ["warn", { argsIgnorePattern: "^_" }],
        },
    },
    {
        ignores: [".routify/**", "dist/**", "node_modules/**"],
    },
)
