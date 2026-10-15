/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'


const PORT_DEV = 1430
const PORT_HMR = 1431
const host = process.env.TAURI_DEV_HOST


export default defineConfig(() => ({
    plugins: [vue()],
    clearScreen: false,
    test: {
        environment: 'happy-dom',
        include: ['tests/unit/**/*.test.ts'],
    },
    server: {
        port: PORT_DEV,
        strictPort: true,
        host: host ?? false,
        hmr: host === undefined ? undefined : { protocol: 'ws', host, port: PORT_HMR },
        watch: {
            ignored: ['**/src-tauri/**'],
        },
    },
    build: {
        target: 'esnext',
        cssMinify: true,
        sourcemap: false,
    },
}))
