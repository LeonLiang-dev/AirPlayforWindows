import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    // Strip crossorigin from script and link tags.
    // Tauri 2 serves all assets from a custom protocol where CORS
    // attributes cause the browser to block the resource load.
    {
      name: 'tauri-strip-crossorigin',
      enforce: 'post',
      transformIndexHtml(html) {
        return html.replace(/ crossorigin/g, '');
      },
    },
  ],

  clearScreen: false,

  server: {
    port: 5173,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },

  base: './',
})
