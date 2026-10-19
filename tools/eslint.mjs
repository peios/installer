// Shared PSC JavaScript rules; the Peios brace rule is copied with provenance.
import peios from "./eslint-peios.mjs";

export default [
    {
        files: ["**/*.{js,mjs}"],
        plugins: { peios },
        linterOptions: { reportUnusedDisableDirectives: "error" },
        rules: {
            "peios/braces": "error",
            "no-dupe-args": "error",
            "no-duplicate-case": "error",
            "no-unreachable": "error",
            "no-unused-vars": "error",
            "prefer-const": "error",
        },
    },
];
