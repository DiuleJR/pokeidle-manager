import { describe, expect, it } from 'vitest'
import { CombatAssetResolver, emptyCatalog, ItemAssetResolver, PokemonAssetResolver, type GameItemCatalog } from './assets'

const catalog: GameItemCatalog = {
  ...emptyCatalog,
  items: {
    '200': { id: 200, name: 'Small Potion', category: 'heal', assetPath: 'assets/site/assets/markitems/small_potion.png' },
    '40': { id: 40, name: 'Earth Stone', category: 'stone', assetPath: 'assets/site/assets/items/earth_stone.gif' },
    '70011': { id: 70011, name: 'Fragmento de Chave', category: 'fragment', assetPath: 'img/itens/fragmento-chave.png' },
    '70013': { id: 70013, name: 'Fragmento de Bicicleta', category: 'fragment', assetPath: 'img/itens/fragmento-bicicleta.png' },
  },
  markerAtlas: {
    assetPath: 'assets/site/assets/maps/marker-atlas.png', cell: 64, cols: 16, rows: 50,
    slots: { '401': [12, 2], '402': [13, 2] },
  },
}

describe('real game asset resolvers', () => {
  it('uses the source catalog for names, categories and item assets', () => {
    const potion = ItemAssetResolver.describe({ id: '200', assetKey: '200', name: 'Item #200', quantity: 4, category: 'other' }, catalog)
    expect(potion).toMatchObject({ name: 'Small Potion', category: 'potions' })
    expect(ItemAssetResolver.assetPath(potion, catalog)).toContain('small_potion')
    expect(ItemAssetResolver.describe({ id: '999', name: 'Item #999', quantity: 1, category: 'other' }, catalog).name).toBe('Item #999')
  })

  it('resolves the confirmed Bicycle Fragment name and first-party sprite', () => {
    const fragment = ItemAssetResolver.describe({ id: '70013', name: 'Item #70013', quantity: 1, category: 'other' }, catalog)
    expect(fragment).toMatchObject({ name: 'Fragmento de Bicicleta', category: 'other' })
    expect(ItemAssetResolver.assetPath(fragment, catalog)).toBe('img/itens/fragmento-bicicleta.png')
  })

  it('resolves the confirmed Key Fragment name and first-party sprite', () => {
    const fragment = ItemAssetResolver.describe({ id: '70011', name: 'Item #70011', quantity: 1, category: 'other' }, catalog)
    expect(fragment).toMatchObject({ name: 'Fragmento de Chave', category: 'other' })
    expect(ItemAssetResolver.assetPath(fragment, catalog)).toBe('img/itens/fragmento-chave.png')
  })

  it('keeps ball paths separate from item ids and supplies the real priority icons', () => {
    const ball = { id: 'ball-4', assetKey: 'ball-4', name: 'Ultra Ball', quantity: 1, category: 'balls' as const }
    expect(ItemAssetResolver.assetPath(ball, catalog)).toContain('ball-ultra')
    expect(CombatAssetResolver.ball(4)).toContain('ball-ultra')
    expect(CombatAssetResolver.ball(5)).toContain('ball-beast')
    expect(CombatAssetResolver.ball(999)).toBeNull()
  })

  it('prefers lookShiny and otherwise resolves the normal looktype', () => {
    const shiny = { id: '1', name: 'Pupitar', level: 1, types: [], locked: false, shiny: true, looktype: 401, lookShiny: 402 }
    expect(PokemonAssetResolver.preferredLooktypes(shiny)).toEqual([402, 401])
    expect(PokemonAssetResolver.atlasSlot(shiny, catalog.markerAtlas)).toEqual({ looktype: 402, slot: [13, 2] })
    expect(PokemonAssetResolver.atlasSlot({ ...shiny, lookShiny: 999 }, catalog.markerAtlas)).toEqual({ looktype: 401, slot: [12, 2] })
  })
})
