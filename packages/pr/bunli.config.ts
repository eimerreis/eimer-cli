import { defineConfig } from '@bunli/core'

export default defineConfig({
  name: 'pr',
  version: '0.1.0',
  description: 'CLI to create pull requests',
  
  commands: {
    directory: './src/commands'
  },
  
  build: {
    entry: './src/index.ts',
    outdir: './dist',
    targets: ['native'],
    minify: true,
    sourcemap: true,
    compress: false
  },
  
  dev: {
    watch: true,
    inspect: true
  },
  
  test: {
    pattern: ['**/*.test.ts', '**/*.spec.ts'],
    coverage: true,
    watch: false
  },

  plugins: [],
})
