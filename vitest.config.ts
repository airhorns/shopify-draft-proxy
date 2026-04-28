import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    setupFiles: ['tests/support/integration-runtime.setup.ts'],
  },
});
