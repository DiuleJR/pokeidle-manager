import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'node:url'

const projectRoot = fileURLToPath(new URL('.', import.meta.url))
const mobileZeroTierIp = process.env.POKEIDLE_MOBILE_ZEROTIER_IP
const mobileZeroTierOrigin = mobileZeroTierIp ? `http://${mobileZeroTierIp}:1420` : null

const mobileZeroTierOriginGuard = mobileZeroTierOrigin
  ? {
      name: 'mobile-zerotier-origin-guard',
      configureServer(server: import('vite').ViteDevServer) {
        server.middlewares.use('/api/v1/mobile', (request, response, next) => {
          const origin = request.headers.origin
          if (origin && origin !== mobileZeroTierOrigin) {
            response.statusCode = 403
            response.end('Forbidden')
            return
          }
          next()
        })
      },
    }
  : null

export default defineConfig({
  plugins: [react(), ...(mobileZeroTierOriginGuard ? [mobileZeroTierOriginGuard] : [])],
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    ...(mobileZeroTierIp ? { allowedHosts: [mobileZeroTierIp] } : {}),
    proxy: {
      '/api/v1/mobile': {
        target: 'http://127.0.0.1:1421',
        changeOrigin: true,
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyRequest) => {
            proxyRequest.setHeader('origin', 'http://127.0.0.1:1420')
          })
        },
      },
    },
  },
  build: {
    sourcemap: false,
    rollupOptions: {
      input: {
        desktop: `${projectRoot}/index.html`,
        mobile: `${projectRoot}/mobile.html`,
      },
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  test: { environment: 'jsdom', globals: true, setupFiles: ['./src/test-setup.ts'] },
})
