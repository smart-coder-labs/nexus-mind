import { defineConfig } from 'vitest/config'
import { loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { resolve } from 'path'

// Mirrors `AdminProfile` in src/config/disabled-sections.ts. Validated HERE, at
// build time, because Vite only inlines the string: the runtime check in that
// module would surface a typo as a blank panel in the customer's browser, not
// as a failed deploy.
const ADMIN_PROFILES = ['full', 'only-context']

export default defineConfig(({ mode }) => {
  const profile = loadEnv(mode, process.cwd(), 'VITE_').VITE_ADMIN_PROFILE
  if (profile && !ADMIN_PROFILES.includes(profile)) {
    throw new Error(`Unknown VITE_ADMIN_PROFILE "${profile}". Expected one of: ${ADMIN_PROFILES.join(', ')}`)
  }
  return config
})

const config = {
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  server: {
    port: 3000,
    proxy: {
      '/v1': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    css: false,
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
  },
}
