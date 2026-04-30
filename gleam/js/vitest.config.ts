import { defineConfig } from 'vitest/config';

// Standalone vitest config. The parent workspace's config references
// integration-only setup files; this shim is self-contained, so we
// short-circuit here.
export default defineConfig({
  test: {
    include: ['test/**/*.test.ts'],
    setupFiles: [],
    globals: false,
  },
});
