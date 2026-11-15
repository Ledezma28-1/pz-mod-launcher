import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { readFileSync } from 'node:fs';

const version: string = JSON.parse(readFileSync(new URL('./src-tauri/tauri.conf.json', import.meta.url), 'utf8')).version;

export default defineConfig({
  plugins: [svelte()],
  define: { __VERSION__: JSON.stringify(version) },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: { ignored: ['**/target/**', '**/core/**', '**/src-tauri/**'] },
  },
  build: { target: 'chrome120', outDir: 'dist', emptyOutDir: true },
});
