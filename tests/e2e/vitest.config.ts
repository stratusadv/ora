import { resolve } from 'node:path'
import { defineConfig } from 'vitest/config'


export default defineConfig({
    test: {
        environment: 'node',
        fileParallelism: false,
        hookTimeout: 60_000,
        include: ['tests/e2e/**/*.e2e.ts'],
        root: resolve(import.meta.dirname, '../..'),
        testTimeout: 60_000,
    },
})
