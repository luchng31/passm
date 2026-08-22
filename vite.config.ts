import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // cargo writes into target/ while vite dev runs; watching it crashes
      // with EBUSY on Windows.
      ignored: ['**/target/**', '**/dist/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
  },
});