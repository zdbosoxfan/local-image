import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  base: '/frontend-assets/',
  build: {
    outDir: '../backend/frontend_dist',
    emptyOutDir: true,
    manifest: true,
    sourcemap: false,
    target: 'es2022',
    rollupOptions: { input: 'src/main.tsx' },
  },
});
