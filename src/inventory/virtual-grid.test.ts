import { describe, expect, it } from 'vitest'
import { POKEMON_GRID_ROW_HEIGHT, pokemonGridWindow } from './virtual-grid'

describe('Pokemon inventory grid window', () => {
  it('renders only visible rows and a small overscan for a large inventory', () => {
    const window = pokemonGridWindow(2_000, 900, 700, 0)

    expect(window.columns).toBe(4)
    expect(window.startIndex).toBe(0)
    expect(window.endIndex).toBeLessThan(40)
    expect(window.totalHeight).toBeGreaterThan(100_000)
  })

  it('moves the rendered window while preserving the full scroll height', () => {
    const atTop = pokemonGridWindow(2_000, 900, 700, 0)
    const scrolled = pokemonGridWindow(2_000, 900, 700, POKEMON_GRID_ROW_HEIGHT * 30)

    expect(scrolled.startIndex).toBeGreaterThan(atTop.startIndex)
    expect(scrolled.top).toBeGreaterThan(0)
    expect(scrolled.totalHeight).toBe(atTop.totalHeight)
    expect(scrolled.endIndex - scrolled.startIndex).toBeLessThan(40)
  })

  it('keeps a single column on narrow viewports and an empty range for no Pokemon', () => {
    expect(pokemonGridWindow(10, 200, 400, 0).columns).toBe(1)
    expect(pokemonGridWindow(0, 200, 400, 0)).toMatchObject({
      startIndex: 0,
      endIndex: 0,
      totalHeight: 0,
    })
  })

  it('clamps an old scroll position after filtering to a shorter list', () => {
    const window = pokemonGridWindow(5, 900, 700, POKEMON_GRID_ROW_HEIGHT * 100)

    expect(window.startIndex).toBe(5)
    expect(window.endIndex).toBe(5)
    expect(window.top).toBe(window.totalHeight)
  })
})
