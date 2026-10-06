import type { MarketItemMetadata } from '../types'
import type { GameItemCatalog } from './assets'

/**
 * Market metadata enriches the shared catalog but is never allowed to erase a
 * known asset when a sparse `market.item` ficha omits `icone`.
 */
export function mergeMarketCatalog(
  baseCatalog: GameItemCatalog,
  metadataEntries: MarketItemMetadata[],
): GameItemCatalog {
  const items = { ...baseCatalog.items }
  for (const metadata of metadataEntries) {
    const known = items[String(metadata.itemId)]
    items[String(metadata.itemId)] = {
      id: metadata.itemId,
      name: metadata.name || known?.name || `Item #${metadata.itemId}`,
      category: metadata.category ?? known?.category ?? null,
      // ID 5 is a separate Market namespace. Its controlled Beast Ball
      // fallback is chosen by marketItemForId when this path is absent.
      assetPath: metadata.itemId === 5
        ? metadata.assetPath
        : metadata.assetPath ?? known?.assetPath ?? null,
    }
  }
  return { ...baseCatalog, items }
}
