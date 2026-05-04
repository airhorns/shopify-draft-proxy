import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    hookTimeout: 30_000,
    setupFiles: ['tests/support/integration-runtime.setup.ts'],
    testTimeout: 30_000,
  },
});
