import { describe, expect, it } from 'vitest'
import { currentMarketPrice, MARKET_PRICE_STALE_AFTER_MS } from './market-price'
import type { MarketSummary } from './types'

const summary: MarketSummary = {
  itemId: 200,
  listings: 2,
  units: 8,
  minGold: 12_450,
  minOrb: null,
  minGoldKnown: true,
  minOrbKnown: true,
  observedAt: 1_000,
}

describe('currentMarketPrice', () => {
  it('distinguishes prices, explicit no-offer, not loaded, and unknown fields', () => {
    expect(currentMarketPrice(summary, 'gold', 1_001)).toEqual({
      state: 'available',
      price: 12_450,
    })
    expect(currentMarketPrice(summary, 'orb', 1_001)).toEqual({ state: 'no-offer' })
    expect(currentMarketPrice(undefined, 'gold', 1_001)).toEqual({ state: 'not-loaded' })
    expect(currentMarketPrice({ ...summary, minOrbKnown: false }, 'orb', 1_001)).toEqual({
      state: 'unknown',
    })
  })

  it('keeps old values visible but marks them stale after the Reader watchdog window', () => {
    expect(currentMarketPrice(summary, 'gold', 1_000 + MARKET_PRICE_STALE_AFTER_MS)).toEqual({
      state: 'available',
      price: 12_450,
    })
    expect(currentMarketPrice(summary, 'gold', 1_001 + MARKET_PRICE_STALE_AFTER_MS)).toEqual({
      state: 'stale',
      price: 12_450,
      hasData: true,
    })
    expect(
      currentMarketPrice(
        { ...summary, minGoldKnown: false },
        'gold',
        1_001 + MARKET_PRICE_STALE_AFTER_MS,
      ),
    ).toEqual({ state: 'stale', price: null, hasData: false })
  })
})
