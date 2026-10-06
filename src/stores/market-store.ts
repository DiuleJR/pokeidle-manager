import { create } from 'zustand'
import type { MarketSnapshot } from '../types'

type MarketStore = {
  snapshot: MarketSnapshot | null
  setSnapshot: (snapshot: MarketSnapshot) => void
}

export const useMarketStore = create<MarketStore>((set) => ({
  snapshot: null,
  setSnapshot: (snapshot) =>
    set((state) => (state.snapshot?.version === snapshot.version ? {} : { snapshot })),
}))
