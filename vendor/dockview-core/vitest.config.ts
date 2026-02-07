import { defineConfig } from 'vitest/config';

export default defineConfig({
    test: {
        environment: 'jsdom',
        globals: true,
        include: ['src/__tests__/**/*.spec.ts'],
        exclude: ['**/node_modules/**'],
        setupFiles: [
            'src/__tests__/__mocks__/vitest-jest-compat.ts',
            'src/__tests__/__mocks__/resizeObserver.js',
        ],
        alias: {
            '^(\\.{1,2}/.*)\\.js$': '$1',
        },
    },
});
