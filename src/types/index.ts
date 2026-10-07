export type AccountStatus = 'offline' | 'connecting' | 'online' | 'login_required' | 'error'
export type AccountMode = 'background' | 'browser' | 'transitioning'
export type PendingNavigation = { kind: 'hunt'; slug: string } | { kind: 'center' }
export type AutoBuyRule = {
  kind: 'item' | 'ball'
  itemId: number
  minimum: number
  quantity: number
  enabled: boolean
  status?: string
}
export type MarketCurrency = 'gold' | 'orb'
export type MarketSummary = {
  itemId: number
  listings: number
  units: number
  minGold: number | null
  minOrb: number | null
}
export type MarketSniperRule = {
  id: string
  accountId: string
  targetType: 'item'
  itemId: number
  enabled: boolean
  currency: MarketCurrency
  maxPrice: number
  /** Legacy persisted field; current rules buy the affordable quantity from each listing. */
  quantity?: number
  budget: number
  minimumBalance: number
  spent: number
  purchasedQuantity: number
}
export type MarketPurchase = {
  id: string
  purchasedAt: number
  accountId: string
  ruleId: string
  listingId: number
  itemId: number
  description: string
  quantity: number
  total: number
  currency: MarketCurrency
  outcome: 'purchased' | 'lostLottery' | 'uncertain'
}
export type MarketCandidate = {
  listingId: number
  accountId: string
  ruleId: string
  itemId: number
  name: string
  quantity: number
  unitPrice: number
  total: number
  currency: MarketCurrency
  purchasableAt: number
  status: string
}
export type MarketHistoryPokemon = {
  name: string
  level: number
  looktype: number
  shiny: boolean
  lookShiny: number | null
}
export type MarketHistoryEntry = {
  id: number
  occurredAt: number
  kind: string
  currency: MarketCurrency
  description: string
  total: number
  seller: string
  buyer: string
  itemName: string | null
  quantity: number | null
  pokemon: MarketHistoryPokemon | null
}
export type MarketTopItemSale = {
  itemName: string
  quantity: number
  transactions: number
  currency: MarketCurrency | null
  averageUnitPrice: number | null
  averageGoldUnitPrice: number | null
  averageOrbUnitPrice: number | null
}
export type MarketItemMetadata = {
  itemId: number
  name: string
  category: string | null
  assetPath: string | null
  updatedAt: number
}
export type MarketSnapshot = {
  version: number
  readerAccountId: string | null
  readerStatus: string
  lastMarketError: string | null
  lastUpdatedAt: number | null
  summaries: MarketSummary[]
  itemMetadata: MarketItemMetadata[]
  rules: MarketSniperRule[]
  purchases: MarketPurchase[]
  purchaseHistoryTruncated: boolean
  historyStatus: string
  historyLastUpdatedAt: number | null
  recentTransactions: MarketHistoryEntry[]
  topItemSales: MarketTopItemSale[]
  transactionHistoryConfirmed: boolean
}
export type AutomationKind =
  | 'autoPotion'
  | 'autoRevive'
  | 'ballUntilCapture'
  | 'ballContinuous'
  | 'autoVendaLoot'
  | 'returnToHunt'
  | 'autoBuyPotion'
  | 'autoBuyBall'

/** Revisions are scoped to one account and let the UI cache heavy domains safely. */
export interface AccountDomainRevisions {
  depot: number
  inventory: number
  hunt_options: number
}

export type AccountDomain = keyof AccountDomainRevisions

export interface AccountDomainData {
  depot: DepotPokemon[]
  inventory: InventoryItem[]
  hunt_options: NonNullable<AccountView['huntOptions']>
}

/** Response returned by a revision-aware, on-demand account-domain request. */
export interface AccountDomainResponse<D extends AccountDomain = AccountDomain> {
  accountId: string
  revision: number
  changed: boolean
  data?: AccountDomainData[D] | null
}

export interface AccountDomainCacheEntry<D extends AccountDomain = AccountDomain> {
  revision: number
  data: AccountDomainData[D]
}

export interface AccountView {
  id: string
  /** Present for lightweight runtime summaries; omitted by legacy/mock fixtures. */
  domainRevisions?: AccountDomainRevisions
  nick: string
  color: string
  status: AccountStatus
  mode: AccountMode
  runtime?:
    | 'offline'
    | 'browser_bootstrap'
    | 'waiting_for_login'
    | 'browser_connected'
    | 'preparing_handoff'
    | 'background_connecting'
    | 'background'
    | 'reconnecting'
    | 'login_required'
    | 'error'
    | 'stopping'
  level: number
  trainerLevel?: number
  activePokemon?: {
    id: string
    name: string
    level: number
    speciesId?: number
    quality?: number
    potency?: number
    shiny?: boolean
    looktype?: number
    lookShiny?: number
    heldItemId?: number
    types: string[]
    ivs?: {
      hp?: number
      atk?: number
      def?: number
      spAtk?: number
      spDef?: number
      speed?: number
    }
  }
  xpBonus?: {
    eventTrainerPct?: number
    eventPokemonPct?: number
    guildRankPct?: number
    guildBoostActive?: boolean
    twitchPct?: number
  }
  xp: number
  xpLevel: number
  xpNext: number
  power: number
  quality: number
  types: string[]
  vip?: boolean
  vipExpiresAt?: number
  vipDataAvailable?: boolean
  combatLockUntil?: number
  serverOffsetMs?: number
  combatLocked?: boolean
  commandTransportAvailable?: boolean
  pendingNavigation?: PendingNavigation
  navigationError?: string
  automationError?: string
  autoBuyRules?: AutoBuyRule[]
  captureMode?: 'until_capture' | 'continuous' | 'off'
  captureQueueLen?: number
  captureError?: string
  huntOptions?: {
    slug: string
    name: string
    area?: string
    level?: number
    total_spawns?: number
    region?: string
    looktype?: number
    species?: { speciesId?: number; weight?: number }[]
  }[]
  pendingHunt?: string
  activity?: 'farming' | 'pokemon_center' | 'idle' | 'unknown'
  map: string
  onlineSeconds: number
  huntStartedAtMs?: number | null
  pokemon: string
  hp: number
  maxHp: number
  xpPerHour: number
  goldPerHour: number
  gold: number
  gems: number
  diamonds: number
  potions: number
  potionName?: string
  activePotionId?: string
  potionUsageSource?: 'inventory_delta'
  balls: number
  ballName?: string
  activeBallId?: string
  hunt: string | null
  wildPokemon: { name: string; level: number; hp: number; maxHp: number } | null
  kills: number
  captures: number
  xpGained: number
  goldGained: number
  drops: string[]
  potionsPerHour: number
  potionUsagePerHour?: Record<string, number>
  ballsPerHour: number
  revives: number
  otherItems: number
  automations: Record<AutomationKind, boolean>
  potionThreshold: number
  potionIds?: string[]
  ballIds?: string[]
  inventory: InventoryItem[]
  depot: DepotPokemon[]
  depotLocks: DepotLocks
}

export type InventoryCategory = 'potions' | 'balls' | 'stones' | 'other'
export interface InventoryItem {
  id: string
  /** Raw game ID used for catalog lookup. Balls keep their `ball-` UI identity. */
  assetKey?: string
  name: string
  quantity: number
  category: InventoryCategory
}
export interface DepotPokemon {
  id: string
  name: string
  level: number
  power?: number
  quality?: number
  note?: number
  ivTotal?: number
  types: string[]
  locked: boolean
  shiny?: boolean
  speciesId?: number
  looktype?: number
  lookShiny?: number
  p5?: boolean
  caughtAt?: number
}
export interface DepotLocks {
  autoLockShiny: boolean
  autoLockNota9: boolean
  autoLockNotaMin: number
  autoLockP5: boolean
}

export interface AppSettings {
  mockMode: boolean
  developerMode: boolean
  minimizeToTray: boolean
  gameUrl: string
  startupBrowserConcurrency: number
}

export type Page = 'Dashboard' | 'Automações' | 'Inventários' | 'Mercado' | 'Onde Caçar' | 'Configurações'
