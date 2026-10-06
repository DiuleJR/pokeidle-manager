import { describe, expect, it, vi } from 'vitest'
import { createPokemonSpriteResolver, type PokemonSpriteFrame } from './sprite-cache'

const frame: PokemonSpriteFrame = {
  assetPath: 'assets/pokemon.png',
  x: 0,
  y: 0,
  width: 32,
  height: 32,
  pageWidth: 256,
  pageHeight: 256,
}

describe('Pokemon sprite request cache', () => {
  it('shares pending and completed looktype requests across overlapping fallbacks', async () => {
    const resolvers = new Map<number, (value: PokemonSpriteFrame | null) => void>()
    const load = vi.fn((looktype: number) => {
      if (looktype === 900) return Promise.resolve(null)
      return new Promise<PokemonSpriteFrame | null>((done) => resolvers.set(looktype, done))
    })
    const resolveSprite = createPokemonSpriteResolver(load)
    const shinyFallback = resolveSprite([900, 401])
    const normal = resolveSprite([401])

    expect(load).toHaveBeenCalledTimes(2)
    resolvers.get(401)?.(frame)
    await Promise.resolve()

    await expect(shinyFallback).resolves.toEqual(frame)
    await expect(normal).resolves.toEqual(frame)
    expect(load).toHaveBeenCalledTimes(2)
  })

  it('retries a looktype after a transient resolver failure', async () => {
    const load = vi
      .fn<(looktype: number) => Promise<PokemonSpriteFrame | null>>()
      .mockRejectedValueOnce(new Error('temporary'))
      .mockResolvedValueOnce(frame)
    const resolveSprite = createPokemonSpriteResolver(load)

    await expect(resolveSprite([401])).rejects.toThrow('temporary')
    await expect(resolveSprite([401])).resolves.toEqual(frame)
    expect(load).toHaveBeenCalledTimes(2)
  })
})
