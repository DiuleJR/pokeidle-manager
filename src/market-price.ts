import type { MarketCurrency, MarketSummary } from './types'

// Matches the Market Reader's existing 90-second stale watchdog. The global
// Market page already refreshes its snapshot, so this adds no timer or query.
export const MARKET_PRICE_STALE_AFTER_MS = 90_000

export type CurrentMarketPrice =
  | { state: 'not-loaded' }
  | { state: 'unknown' }
  | { state: 'no-offer' }
  | { state: 'available'; price: number }
  | { state: 'stale'; price: number | null; hasData: boolean }

export function currentMarketPrice(
  summary: MarketSummary | undefined,
  currency: MarketCurrency,
  nowMs: number,
): CurrentMarketPrice {
  if (!summary) return { state: 'not-loaded' }

  const known = currency === 'gold' ? summary.minGoldKnown : summary.minOrbKnown
  const price = currency === 'gold' ? summary.minGold : summary.minOrb
  if (nowMs - summary.observedAt > MARKET_PRICE_STALE_AFTER_MS) {
    return { state: 'stale', price: known ? price : null, hasData: known }
  }
  if (!known) return { state: 'unknown' }
  if (price === null) return { state: 'no-offer' }
  return { state: 'available', price }
}
