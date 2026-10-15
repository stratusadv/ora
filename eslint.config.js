import js from '@eslint/js'
import { defineConfig, globalIgnores } from 'eslint/config'
import globals from 'globals'
import pluginVue from 'eslint-plugin-vue'
import tseslint from 'typescript-eslint'

export default defineConfig(
    globalIgnores([
        'dist/**',
        'node_modules/**',
        'public/**',
        'src-tauri/**',
    ]),
    {
        files: ['**/*.{ts,vue}'],
        extends: [
            js.configs.recommended,
            tseslint.configs.strictTypeChecked,
            tseslint.configs.stylisticTypeChecked,
            pluginVue.configs['flat/recommended'],
        ],
        languageOptions: {
            ecmaVersion: 2022,
            globals: globals.browser,
            parserOptions: {
                extraFileExtensions: ['.vue'],
                parser: tseslint.parser,
                projectService: true,
                tsconfigRootDir: import.meta.dirname,
            },
            sourceType: 'module',
        },
        rules: {
            'curly': ['error', 'multi-line'],
            'eqeqeq': ['error', 'always'],
            'no-console': ['error', { allow: ['error', 'warn'] }],
            'no-implicit-coercion': 'error',
            'no-param-reassign': 'error',
            'no-undef': 'off',
            'no-var': 'error',
            'prefer-const': 'error',

            '@typescript-eslint/no-invalid-void-type': [
                'error',
                { allowInGenericTypeArguments: true },
            ],
            '@typescript-eslint/prefer-readonly': 'error',
            '@typescript-eslint/restrict-template-expressions': ['error', { allowNumber: true }],
            '@typescript-eslint/strict-boolean-expressions': 'error',
            '@typescript-eslint/switch-exhaustiveness-check': 'error',

            'vue/block-lang': ['error', { script: { lang: 'ts' } }],
            'vue/component-api-style': ['error', ['script-setup']],
            'vue/define-emits-declaration': ['error', 'type-based'],
            'vue/define-macros-order': 'error',
            'vue/define-props-declaration': ['error', 'type-based'],
            'vue/html-indent': ['error', 4],
            'vue/html-self-closing': [
                'error',
                {
                    html: { void: 'always', normal: 'always', component: 'always' },
                    svg: 'always',
                    math: 'always',
                },
            ],
            'vue/max-attributes-per-line': [
                'error',
                {
                    singleline: { max: Number.POSITIVE_INFINITY },
                    multiline: { max: 1 },
                },
            ],
            'vue/max-len': [
                'error',
                {
                    code: 100,
                    template: 100,
                    ignoreComments: false,
                    ignoreHTMLAttributeValues: true,
                    ignoreHTMLTextContents: true,
                    ignoreRegExpLiterals: true,
                    ignoreStrings: true,
                    ignoreTemplateLiterals: true,
                    ignoreUrls: true,
                },
            ],
            'vue/no-undef-components': 'error',
            'vue/no-unused-emit-declarations': 'error',
            'vue/no-unused-properties': 'error',
            'vue/no-unused-refs': 'error',
            'vue/no-useless-v-bind': 'error',
            'vue/prefer-true-attribute-shorthand': 'error',
            'vue/prop-name-casing': 'off',
            'vue/require-typed-ref': 'error',
            'vue/singleline-html-element-content-newline': 'off',
        },
    },
    {
        files: ['**/*.ts'],
        rules: {
            'max-len': [
                'error',
                {
                    code: 100,
                    ignoreRegExpLiterals: true,
                    ignoreStrings: true,
                    ignoreTemplateLiterals: true,
                    ignoreUrls: true,
                },
            ],
        },
    },
    {
        files: ['vite.config.ts'],
        languageOptions: {
            globals: globals.node,
        },
    },
)
