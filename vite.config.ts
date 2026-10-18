import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// 产物由 Rust 侧内置的静态服务器提供（http://127.0.0.1:<port>/），
// 因此这里用相对路径 base，打包目标锁定 es2020（Chromium 132 完全支持）。
export default defineConfig({
  base: './',
  plugins: [vue()],
  build: {
    outDir: 'build_dist',
    emptyOutDir: true,
    target: 'es2020',
    assetsDir: 'assets',
    chunkSizeWarningLimit: 1200,
  },
  server: {
    port: 5173,
    host: '127.0.0.1',
  },
})
