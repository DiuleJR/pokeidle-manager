export type MobileConnectionOwner = 'browser' | 'background' | 'transition' | 'none'

export interface MobilePokemon {
  name: string
  level: number
  hp: number
  maxHp: number
  shiny: boolean
  looktype: number | null
  lookShiny: number | null
}

export interface MobileStackItem {
  id: number
  name: string
  quantity: number
}

export interface MobileAccount {
  id: string
  displayName: string
  status: 'online' | 'offline'
  connectionOwner: MobileConnectionOwner
  level: number | null
  huntName: string | null
  huntElapsedMs: number | null
  activePokemon: MobilePokemon | null
  gold: number | null
  orbs: number | null
  xpPerHour: number
  goldPerHour: number
  potion: MobileStackItem | null
  ball: MobileStackItem | null
  automations: {
    autoPotion: boolean
    autoRevive: boolean
    autoSaleLoot: boolean
    autoReturnHunt: boolean
    autoLockShiny: boolean
    autoLockNota9: boolean
    autoLockP5: boolean
    captureMode: 'until_capture' | 'continuous' | 'off'
    captureQueueLen: number
  }
}

export interface MobileInventoryItem {
  id: string
  name: string
  category: 'potion' | 'ball' | 'stone' | 'other'
  quantity: number
  assetPath: string | null
  accountId?: string
  accountName?: string
}

export interface MobileInventoryPokemon {
  id: string
  name: string
  level: number
  shiny: boolean
  speciesId: number | null
  looktype: number | null
  lookShiny: number | null
  quality: number | null
  note: number | null
  power: number | null
  ivTotal: number | null
  types: string[]
  accountId?: string
  accountName?: string
}

export interface MobileInventoryPage {
  accountId: string
  kind: 'items' | 'pokemon'
  offset: number
  limit: number
  total: number
  items: MobileInventoryItem[]
  pokemon: MobileInventoryPokemon[]
  types: string[]
}

export interface MobilePokemonSprite {
  assetPath: string
  x: number
  y: number
  width: number
  height: number
  pageWidth: number
  pageHeight: number
}

export interface MobileMarketSummary {
  itemId: number
  name: string
  category: string
  listings: number
  units: number
  minGold: number | null
  minOrb: number | null
}

export interface MobileMarketSummaryPage {
  offset: number
  limit: number
  total: number
  categories: string[]
  items: MobileMarketSummary[]
}

export interface MobileMarketTopSale {
  currencyGroup: 'all' | 'gold' | 'gems'
  itemName: string
  quantity: number
  transactions: number
  averageUnitPrice: number | null
  averageGoldUnitPrice: number | null
  averageOrbUnitPrice: number | null
}

export interface MobileMarketTransaction {
  id: string
  occurredAt: number
  kind: 'item' | 'pokemon' | 'unknown'
  currency: 'gold' | 'orb' | 'unknown'
  name: string
  looktype: number | null
  lookShiny: number | null
  shiny: boolean
  total: number
  quantity: number
  unitPrice: number | null
}

export interface MobileResolvedItemAssets {
  items: Array<{
    id: number | null
    name: string | null
    assetPath: string | null
  }>
}

export interface MobileMarket {
  readerStatus: string
  lastUpdatedAt: number | null
  enabledRuleCount: number
  totalRuleCount: number
  summaries: MobileMarketSummary[]
  topItemSales: MobileMarketTopSale[]
  recentTransactions: MobileMarketTransaction[]
}

export interface MobileSnapshot {
  schemaVersion: number
  revision: number
  managerTimestampMs: number
  aggregate: {
    totalAccounts: number
    onlineAccounts: number
    xpPerHourTotal: number
    goldPerHourTotal: number
    goldTotal: number
    orbsTotal: number
  }
  accounts: MobileAccount[]
  market: MobileMarket
}
