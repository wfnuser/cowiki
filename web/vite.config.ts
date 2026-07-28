import path from 'path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const cloudTarget = 'http://127.0.0.1:8787'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  optimizeDeps: {
    include: ['framer-motion'],
  },
  server: {
    proxy: {
      '/api': cloudTarget,
      '/git': cloudTarget,
      '/healthz': cloudTarget,
    }
  }
})
