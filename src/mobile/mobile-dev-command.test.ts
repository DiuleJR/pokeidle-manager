import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

describe('mobile local-server development command', () => {
  it('keeps the Cargo feature in a dedicated npm script', () => {
    const packageJsonPath = resolve(process.cwd(), 'package.json')
    const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8')) as {
      scripts?: Record<string, string>
    }

    expect(packageJson.scripts?.['tauri:dev:mobile']).toBe(
      'tauri dev -- --no-default-features --features community-build,mobile-local-server',
    )
  })
})
