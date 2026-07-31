import path from 'path'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import type { Plugin } from 'vite'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const cloudTarget = 'http://127.0.0.1:8787'
const skillRoot = path.resolve(__dirname, '../skills/cowiki-space')

function skillFiles(directory = skillRoot): string[] {
  return readdirSync(directory).flatMap((name) => {
    const absolutePath = path.join(directory, name)
    return statSync(absolutePath).isDirectory() ? skillFiles(absolutePath) : [absolutePath]
  })
}

function cowikiSkillPlugin(): Plugin {
  const files = skillFiles()
  const skillEntry = path.join(skillRoot, 'SKILL.md')

  return {
    name: 'cowiki-skill',
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const pathname = new URL(request.url || '/', 'http://localhost').pathname
        const bundledPrefix = '/skills/cowiki-space/'
        const relativePath = pathname === '/skill.md'
          ? 'SKILL.md'
          : pathname.startsWith(bundledPrefix)
            ? decodeURIComponent(pathname.slice(bundledPrefix.length))
            : null

        if (!relativePath) {
          next()
          return
        }

        const absolutePath = path.resolve(skillRoot, relativePath)
        if (
          !absolutePath.startsWith(`${skillRoot}${path.sep}`)
          || !files.includes(absolutePath)
        ) {
          next()
          return
        }

        response.statusCode = 200
        response.setHeader(
          'Content-Type',
          absolutePath.endsWith('.md') ? 'text/markdown; charset=utf-8' : 'text/javascript; charset=utf-8',
        )
        response.end(readFileSync(absolutePath))
      })
    },
    generateBundle() {
      this.emitFile({
        type: 'asset',
        fileName: 'skill.md',
        source: readFileSync(skillEntry),
      })
      for (const absolutePath of files) {
        const relativePath = path.relative(skillRoot, absolutePath).split(path.sep).join('/')
        this.emitFile({
          type: 'asset',
          fileName: `skills/cowiki-space/${relativePath}`,
          source: readFileSync(absolutePath),
        })
      }
    },
  }
}

export default defineConfig({
  plugins: [react(), tailwindcss(), cowikiSkillPlugin()],
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
