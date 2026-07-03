// @ts-check
import { defineConfig } from 'astro/config';
import tailwindcss from '@tailwindcss/vite';
import react from '@astrojs/react';

export default defineConfig({
  integrations: [react()],
  vite: {
    plugins: [tailwindcss()],
    // Force Vite to pre-bundle the react-dom/client subpath. Without this it is
    // served as raw CommonJS, so the React island renderer's
    // `import { createRoot } from 'react-dom/client'` finds no named export and
    // every island fails to hydrate ("does not provide an export named 'createRoot'").
    optimizeDeps: {
      include: ['react', 'react-dom', 'react-dom/client'],
    },
  },
});
