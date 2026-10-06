import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import type { DepotPokemon, InventoryCategory, InventoryItem } from '../types'

export type CatalogItem = {
  id: number
  name: string
  category: string | null
  assetPath: string | null
}
export type MarkerAtlas = {
  assetPath: string
  cell: number
  cols: number
  rows: number
  slots: Record<string, [number, number]>
}
export type GameItemCatalog = { items: Record<string, CatalogItem>; markerAtlas: MarkerAtlas }

export const emptyCatalog: GameItemCatalog = {
  items: {},
  markerAtlas: { assetPath: '', cell: 64, cols: 16, rows: 1, slots: {} },
}

let catalogRequest: Promise<GameItemCatalog> | null = null
export function useGameItemCatalog() {
  const [catalog, setCatalog] = useState<GameItemCatalog>(emptyCatalog)
  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) return
    catalogRequest ??= invoke<GameItemCatalog>('game_item_catalog')
    void catalogRequest.then(setCatalog).catch(() => setCatalog(emptyCatalog))
  }, [])
  return catalog
}

const ballAssets: Record<string, string> = {
  '1': 'assets/site/assets/ui/ball-poke.png',
  '2': 'assets/site/assets/ui/ball-great.png',
  '3': 'img/ball-super.png',
  '4': 'assets/site/assets/ui/ball-ultra.png',
  '5': 'img/ball-beast.png',
}

const asInventoryCategory = (category: string | null | undefined): InventoryCategory => {
  if (category === 'heal') return 'potions'
  if (category === 'stone') return 'stones'
  return 'other'
}

export const ItemAssetResolver = {
  describe<T extends InventoryItem>(item: T, catalog: GameItemCatalog): T {
    const key = item.assetKey ?? item.id
    if (key.startsWith('ball-')) return { ...item, category: 'balls' } as T
    const real = catalog.items[key]
    if (!real) return item
    return { ...item, name: real.name || item.name, category: asInventoryCategory(real.category) } as T
  },
  assetPath(item: InventoryItem, catalog: GameItemCatalog) {
    const key = item.assetKey ?? item.id
    if (key.startsWith('ball-')) return ballAssets[key.slice('ball-'.length)] ?? null
    return catalog.items[key]?.assetPath ?? null
  },
}

export const PokemonAssetResolver = {
  preferredLooktypes(pokemon: DepotPokemon) {
    const values = pokemon.shiny ? [pokemon.lookShiny, pokemon.looktype] : [pokemon.looktype]
    return [...new Set(values.filter((value): value is number => typeof value === 'number' && Number.isInteger(value) && value > 0))]
  },
  atlasSlot(pokemon: DepotPokemon, atlas: MarkerAtlas) {
    for (const looktype of this.preferredLooktypes(pokemon)) {
      const slot = atlas.slots[String(looktype)]
      if (slot) return { looktype, slot }
    }
    return null
  },
}

export const CombatAssetResolver = {
  potion(itemId: number, catalog: GameItemCatalog) {
    return catalog.items[String(itemId)]?.assetPath ?? null
  },
  ball(itemId: number) {
    return ballAssets[String(itemId)] ?? null
  },
}
