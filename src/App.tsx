import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CustomTitleBar } from './components/CustomTitleBar'
import { Badge, Button, Card, EmptyState, Switch } from './components/primitives'
import { ItemAssetResolver, type GameItemCatalog, useGameItemCatalog } from './inventory/assets'
import { ItemAsset, PokemonAsset } from './inventory/asset-components'
import { ensureAccountDomain } from './account-domains'
import { mergeMarketCatalog } from './inventory/market-catalog'
import { formatHuntElapsed } from './dashboard-time'
import { POKEMON_GRID_ROW_HEIGHT, pokemonGridWindow } from './inventory/virtual-grid'
import { WhereToHunt } from './hunts/WhereToHunt'
import browserModeIcon from './assets/pokeidle-theme/backgrounds/icon-browser.png'
import backgroundModeIcon from './assets/pokeidle-theme/backgrounds/icon-background.png'
import huntIcon from './assets/pokeidle-theme/backgrounds/icon-hunt.png'
import accountHuntIcon from './assets/pokeidle-theme/backgrounds/icon-account-hunt.svg'
import accountMapIcon from './assets/pokeidle-theme/backgrounds/icon-account-map.svg'
import accountLevelIcon from './assets/pokeidle-theme/backgrounds/icon-level.png'
import gameGoldIcon from './assets/pokeidle-theme/backgrounds/icon-gold.png'
import diamondCurrencyIcon from './assets/pokeidle-theme/backgrounds/icon-diamond.png'
import vipIcon from './assets/pokeidle-theme/backgrounds/icon-vip.svg'
import sidebarLogo from './assets/pokeidle-theme/backgrounds/sidebar-brand.png'
import sidebarWallpaper from './assets/pokeidle-theme/backgrounds/sidebar-wallpaper.png'
import sharedHeaderScenery from './assets/pokeidle-theme/backgrounds/shared-header-scenery.png'
import sharedHeaderTitleboard from './assets/pokeidle-theme/backgrounds/shared-header-titleboard.png'
import menuDashboardIcon from './assets/pokeidle-theme/backgrounds/menu-dashboard.png'
import menuHuntIcon from './assets/pokeidle-theme/backgrounds/menu-hunt.png'
import menuAutomationIcon from './assets/pokeidle-theme/backgrounds/menu-automation.png'
import menuInventoryIcon from './assets/pokeidle-theme/backgrounds/menu-inventory.png'
import menuMarketIcon from './assets/pokeidle-theme/backgrounds/menu-market.png'
import menuSettingsIcon from './assets/pokeidle-theme/backgrounds/menu-settings.png'
import {
  aggregateItems,
  depotPokemon,
  filterItems,
  filterPokemon,
  type PokemonSort,
} from './inventory/selectors'
import { selectVisibleAccounts, useAppStore } from './stores/app-store'
import { useMarketStore } from './stores/market-store'
import type {
  AccountView,
  AutomationKind,
  DepotPokemon,
  InventoryCategory,
  InventoryItem,
  MarketCurrency,
  MarketHistoryEntry,
  MarketSnapshot,
  MarketSniperRule,
  Page,
} from './types'
import {
  accountFromLive,
  type IntegrationDiagnostic,
  type IntegrationLiveSnapshot,
  type LiveRuntimeSnapshot,
} from './real-account'

const pages: Page[] = ['Dashboard', 'Onde Caçar', 'Automações', 'Inventários', 'Mercado', 'Configurações']
type BrowserAccountCommand = 'open_account_browser' | 'reconnect_browser_control' | 'login_account'
type BrowserIntent =
  | { kind: 'add' }
  | { kind: 'account'; accountId: string; command: BrowserAccountCommand }
type BrowserIssue = 'notFound' | 'invalidInstallation' | 'error'
type StartAccountResult =
  | { status: 'started'; accountId: string }
  | { status: 'browserNotFound' | 'invalidInstallation' | 'limitReached' }

function browserIssueFromError(error: unknown): BrowserIssue | null {
  const message = String(error)
  if (message.includes('BROWSER_NOT_FOUND')) return 'notFound'
  if (message.includes('BROWSER_INVALID_INSTALLATION')) return 'invalidInstallation'
  return null
}

function activePokemonAsDepot(account: AccountView): DepotPokemon | undefined {
  const pokemon = account.activePokemon
  if (!pokemon) return undefined
  return {
    id: pokemon.id,
    name: pokemon.name,
    level: pokemon.level,
    speciesId: pokemon.speciesId,
    looktype: pokemon.looktype,
    lookShiny: pokemon.lookShiny,
    shiny: pokemon.shiny,
    power: pokemon.potency,
    quality: pokemon.quality,
    types: pokemon.types,
    locked: false,
  }
}

function selectedItemAsInventory(
  account: AccountView,
  kind: 'potion' | 'ball',
): InventoryItem | undefined {
  if (kind === 'potion' && account.activePotionId) {
    return {
      id: account.activePotionId,
      assetKey: account.activePotionId,
      name: account.potionName ?? 'Poção',
      quantity: account.potions,
      category: 'potions',
    }
  }
  if (kind === 'ball' && account.activeBallId) {
    return {
      id: account.activeBallId,
      assetKey: account.activeBallId,
      name: account.ballName ?? 'Pokébola',
      quantity: account.balls,
      category: 'balls',
    }
  }
  return undefined
}

function BrowserRequiredDialog({ issue, busy, notice, onDownload, onRetry, onCancel }: {
  issue: BrowserIssue
  busy: boolean
  notice: string | null
  onDownload: () => void
  onRetry: () => void
  onCancel: () => void
}) {
  const description = issue === 'invalidInstallation'
    ? 'Foi encontrado um Brave, mas a instalação não parece válida. Reinstale-o pelo site oficial e tente novamente.'
    : issue === 'error'
      ? 'Não foi possível verificar o navegador agora. Você pode tentar novamente.'
      : 'O Pokeidle Manager utiliza o Brave para abrir e gerenciar suas contas. O Brave não foi encontrado neste computador.'

  return (
    <div className="browser-required-layer" role="presentation">
      <section className="card browser-required-dialog" role="dialog" aria-modal="true" aria-labelledby="browser-required-title" aria-describedby="browser-required-description">
        <p className="eyebrow">NAVEGADOR</p>
        <h2 id="browser-required-title">Navegador necessário</h2>
        <p id="browser-required-description">{description}</p>
        {issue === 'notFound' && <p className="browser-required-retry-hint">Instale o Brave e volte aqui para tentar novamente.</p>}
        {notice && <p className="browser-required-notice" role="status">{notice}</p>}
        <div className="dialog-actions browser-required-actions">
          <Button onClick={onDownload} disabled={busy}>Baixar Brave</Button>
          <Button onClick={onRetry} disabled={busy}>{busy ? 'Verificando…' : 'Tentar novamente'}</Button>
          <Button onClick={onCancel} disabled={busy}>Cancelar</Button>
        </div>
      </section>
    </div>
  )
}

function AddAccountFeedback({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <div className="browser-action-error" role="alert">
      <span>{message}</span>
      {onRetry && <Button onClick={onRetry}>Tentar novamente</Button>}
    </div>
  )
}

const sidebarNavIcons: Record<Page, string> = {
  Dashboard: menuDashboardIcon,
  Automações: menuAutomationIcon,
  Inventários: menuInventoryIcon,
  Mercado: menuMarketIcon,
  'Onde Caçar': menuHuntIcon,
  Configurações: menuSettingsIcon,
}

function SidebarNavIcon({ page }: { page: Page }) {
	return <img src={sidebarNavIcons[page]} alt="" aria-hidden="true" />
}

function PageHeader({ title, actions }: { title: string; actions?: ReactNode }) {
  return (
    <header className="shared-page-header">
      <img className="shared-page-header-scenery" src={sharedHeaderScenery} alt="" aria-hidden="true" />
      <div className="shared-page-header-plaque">
        <img src={sharedHeaderTitleboard} alt="" aria-hidden="true" />
        <div className="shared-page-header-copy">
          <h1>{title}</h1>
        </div>
      </div>
      {actions && <div className="shared-page-header-actions">{actions}</div>}
    </header>
  )
}
const automationLabels: Record<AutomationKind, string> = {
  autoPotion: 'Usar Poções',
  autoRevive: 'Usar Revive ao desmaiar',
  ballUntilCapture: 'Lançar até Capturar',
  ballContinuous: 'Lançar sem Parar',
  autoVendaLoot: 'Venda Automática',
  returnToHunt: 'Voltar à hunt ao morrer/reset',
  autoBuyPotion: 'Compra Automática de Poções',
  autoBuyBall: 'Compra Automática de Balls',
}
const automationDescriptions: Record<AutomationKind, string> = {
  autoPotion: 'Usa a poção selecionada no limite de HP.',
  autoRevive: 'Usa Revive quando o ativo desmaia.',
  ballUntilCapture: 'Lança Balls até conseguir uma captura.',
  ballContinuous: 'Mantém lançamentos enquanto estiver ativa.',
  autoVendaLoot: 'Vende loot comum automaticamente.',
  returnToHunt: 'Retoma a Hunt após morte ou reset.',
  autoBuyPotion: 'Repõe as poções selecionadas.',
  autoBuyBall: 'Repõe as Balls selecionadas.',
}
const format = new Intl.NumberFormat('pt-BR')
const MARKET_SNAPSHOT_REFRESH_MS = 10_000
const MARKET_HISTORY_ROW_HEIGHT = 60
const startupDebug = import.meta.env.DEV && import.meta.env.MODE !== 'test'
const communityInfo = {
  version: '0.1.0',
  githubUrl: null as string | null,
  discordUrl: null as string | null,
  pixEnabled: false,
}
const statusFor = (status: AccountView['status']) =>
  ({
    online: 'Online',
    connecting: 'Conectando',
    offline: 'Offline',
    login_required: 'Login necessário',
    error: 'Erro',
  })[status]
const toneFor = (status: AccountView['status']) =>
  status === 'online'
    ? 'positive'
    : status === 'connecting'
      ? 'warning'
      : status === 'error'
        ? 'danger'
        : 'neutral'
const modeFor = (mode: AccountView['mode']) =>
  mode === 'browser' ? 'Navegador' : mode === 'background' ? 'Background' : 'Trocando'
const time = (seconds: number) =>
  `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}min`

type DashboardMetricKind = 'xp' | 'gold' | 'accounts' | 'gems'

function DashboardMetricIcon({ kind }: { kind: DashboardMetricKind }) {
  if (kind === 'accounts') return <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="9" cy="8" r="3" /><circle cx="17" cy="9" r="2.2" /><path d="M3.5 19c.5-3.1 2.4-4.8 5.5-4.8s5 1.7 5.5 4.8M14 18.5c.4-2.2 1.8-3.4 4-3.4 1.5 0 2.6.6 3.3 1.8" /></svg>
  if (kind === 'gems') return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m12 3 7 5-7 13L5 8l7-5Z" /><path d="m5 8 7 4 7-4M12 12V3" /></svg>
  if (kind === 'gold') return <GameGoldIcon />
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m13.4 2-8 12h5.2L9.8 22l8.8-12h-5.2L13.4 2Z" /></svg>
}

function GameGemIcon() {
  return <img className="game-gem-icon" src="https://pokeidle.io/img/moeda-gema.png" alt="" aria-hidden="true" />
}

function GameGoldIcon({ className = '' }: { className?: string }) {
  return (
    <img className={`game-gold-icon ${className}`.trim()} src={gameGoldIcon} alt="" aria-hidden="true" />
  )
}

type AccountInfoIconKind = 'level' | 'gold' | 'gems' | 'diamonds' | 'vip' | 'map' | 'hunt'

function AccountInfoIcon({ kind }: { kind: AccountInfoIconKind }) {
  const icons: Record<AccountInfoIconKind, string> = {
    level: accountLevelIcon,
    gold: gameGoldIcon,
    gems: 'https://pokeidle.io/img/moeda-gema.png',
    diamonds: diamondCurrencyIcon,
    vip: vipIcon,
    map: accountMapIcon,
    hunt: accountHuntIcon,
  }
  return <img src={icons[kind]} alt="" aria-hidden="true" className={`detail-account-info-icon detail-account-info-icon-${kind}`} />
}

function catalogItemByName(catalog: GameItemCatalog, name: string) {
  const normalized = name.trim().toLocaleLowerCase('pt-BR')
  return Object.values(catalog.items).find(
    (item) => item.name.trim().toLocaleLowerCase('pt-BR') === normalized,
  )
}

// `market.itens` has its own item namespace for Balls. In the public generic
// catalog, ID 5 is an unrelated legacy item (Band Aid), while in the Market it
// is the Beast Ball. Keep the override scoped to Market presentation.
function marketItemForId(itemId: number, catalog: GameItemCatalog): InventoryItem {
  const catalogItem = catalog.items[String(itemId)]
  // The public generic catalog calls ID 5 "Band Aid", whereas the Market
  // namespace uses it for Beast Ball. Once `market.item.ficha` is cached it
  // supersedes this local compatibility fallback.
  if (itemId === 5 && (catalogItem?.name !== 'Beast Ball' || !catalogItem.assetPath)) {
    return { id: '5', assetKey: 'ball-5', name: 'Beast Ball', quantity: 0, category: 'balls' }
  }
  if (catalogItem) {
    return {
      id: String(catalogItem.id),
      assetKey: String(catalogItem.id),
      name: catalogItem.name,
      quantity: 0,
      category: 'other',
    }
  }
  // A new Market item must remain purchasable immediately even if the remote
  // visual catalog has not shipped its metadata yet.
  return {
    id: String(itemId),
    assetKey: String(itemId),
    name: `Item #${itemId}`,
    quantity: 0,
    category: 'other',
  }
}

function marketItemForName(name: string, catalog: GameItemCatalog): InventoryItem {
  if (name.trim().toLocaleLowerCase('pt-BR') === 'beast ball') return marketItemForId(5, catalog)
  const catalogItem = catalogItemByName(catalog, name)
  return catalogItem
    ? marketItemForId(catalogItem.id, catalog)
    : { id: name, assetKey: name, name, quantity: 0, category: 'other' }
}

function MarketHistoryAsset({
  entry,
  catalog,
}: {
  entry: MarketHistoryEntry
  catalog: GameItemCatalog
}) {
  if (entry.kind === 'item' && entry.itemName) {
    return <ItemAsset item={marketItemForName(entry.itemName, catalog)} catalog={catalog} compact />
  }
  if (entry.kind === 'pokemon' && entry.pokemon) {
    const pokemon = {
      name: entry.pokemon.name,
      shiny: entry.pokemon.shiny,
      looktype: entry.pokemon.looktype,
      lookShiny: entry.pokemon.lookShiny,
    } as DepotPokemon
    return <PokemonAsset pokemon={pokemon} catalog={catalog} size={32} />
  }
  if (entry.kind === 'diamante')
    return (
      <span className="market-history-diamond" role="img" aria-label="Diamante">
        <img src={diamondCurrencyIcon} alt="" aria-hidden="true" />
      </span>
    )
  return <span className="market-history-kind" aria-hidden="true">🛒</span>
}

function marketHistoryLabel(entry: MarketHistoryEntry) {
  if (entry.kind === 'pokemon') return entry.description || entry.pokemon?.name || 'Pokémon'
  if (entry.kind === 'diamante') return entry.description || 'Diamante'
  return entry.description || 'Item do Mercado'
}

function marketUnitPrice(entry: MarketHistoryEntry) {
  const describedQuantity =
    entry.kind === 'diamante'
      ? entry.description.match(/^\s*([\d.]+)\s*[×x]/i)?.[1]?.replaceAll('.', '')
      : undefined
  const quantity = entry.quantity ?? (describedQuantity ? Number(describedQuantity) : null)
  if (!quantity || quantity < 1) return null
  return Math.floor(entry.total / quantity)
}

function MarketCurrencyIcon({ currency }: { currency: MarketCurrency }) {
  return currency === 'orb' ? <GameGemIcon /> : <DashboardMetricIcon kind="gold" />
}

function AutomationIcon({ kind }: { kind: AutomationKind }) {
  const common = { viewBox: '0 0 24 24', 'aria-hidden': true as const }
  if (kind === 'autoPotion') return <svg {...common}><path d="M9 3h6M10 3v4l-4 5.5A5.2 5.2 0 0 0 10.2 21h3.6a5.2 5.2 0 0 0 4.2-8.5L14 7V3" /><path d="M8.2 15h7.6M12 12v6M9 15h6" /></svg>
  if (kind === 'autoRevive') return <svg {...common}><path d="M12 21s-7-4.4-7-10.4C5 7.7 7 6 9.5 6c1.4 0 2.2.6 2.5 1.3C12.3 6.6 13.1 6 14.5 6 17 6 19 7.7 19 10.6 19 16.6 12 21 12 21Z" /><path d="M12 9v5M9.5 11.5h5" /></svg>
  if (kind === 'ballUntilCapture') return <svg {...common}><circle cx="12" cy="12" r="8.5" /><path d="M3.7 12h16.6M12 9.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5Z" /></svg>
  if (kind === 'ballContinuous') return <svg {...common}><path d="M5 8c1.8-2 4.1-3 7-3 4 0 6.8 2 8 5" /><path d="m17 7 3 3-3 3M19 16c-1.8 2-4.1 3-7 3-4 0-6.8-2-8-5" /><path d="m7 17-3-3 3-3" /></svg>
  if (kind === 'autoVendaLoot' || kind === 'autoBuyPotion' || kind === 'autoBuyBall') return <svg {...common}><path d="M5 7h14l-1 13H6L5 7ZM8 7V4h8v3M9 11h6M9 15h6" /></svg>
  return <svg {...common}><path d="M4 12a8 8 0 1 0 2.3-5.6L4 8.5" /><path d="M4 4v4.5h4.5M12 8v4l2.8 1.8" /></svg>
}

function AccountCard({
  account,
  catalog,
  nowMs,
}: {
  account: AccountView
  catalog: GameItemCatalog
  nowMs: number
}) {
  const open = useAppStore((state) => state.openAccountDetail)
  const hp = account.maxHp ? Math.round((account.hp / account.maxHp) * 100) : 0
  const activePokemon = activePokemonAsDepot(account)
  const potion = selectedItemAsInventory(account, 'potion')
  const ball = selectedItemAsInventory(account, 'ball')
  const huntElapsed = formatHuntElapsed(account.huntStartedAtMs, nowMs)
  return (
    <Card className="account-card premium-account-card clickable" data-status={account.status}>
      <button
        className="card-hit-area"
        aria-label={`Abrir detalhes de ${account.nick}`}
        onClick={() => open(account.id)}
      />
      <header className="premium-account-header">
        <span className="avatar" style={{ background: account.color }}>
          {account.nick[0]}
        </span>
        <div>
          <h3>{account.nick}</h3>
          <div className="premium-account-badges">
            <span className="premium-status">
              <i />
              {statusFor(account.status)}
            </span>
            {account.vip && <span className="vip-badge">VIP</span>}
          </div>
        </div>
        <span className={`mode mode-${account.mode}`}>
          {account.mode === 'browser' && <img src={browserModeIcon} alt="" aria-hidden="true" className="account-mode-mark" />}
          {account.mode === 'background' && <img src={backgroundModeIcon} alt="" aria-hidden="true" className="account-mode-mark" />}
          {account.mode === 'transitioning' && <span className="account-mode-mark" aria-hidden="true">↻</span>}
          {modeFor(account.mode)}
        </span>
      </header>
      <div className="premium-account-body">
        <div className="account-pokemon-visual">
          {activePokemon ? (
            <PokemonAsset pokemon={activePokemon} catalog={catalog} size={160} />
          ) : (
            <span className="account-pokemon-placeholder">?</span>
          )}
        </div>
        <div className="premium-account-details">
          <div className="pokemon">
            <strong>{account.pokemon}</strong>
            <span>Nv. {account.level || '—'}</span>
          </div>
          <div className="hp-row premium-hp-row">
            <span>HP</span>
            <div className="progress">
              <i style={{ width: `${hp}%` }} />
            </div>
            <b>{hp}%</b>
          </div>
          <div className="metric-grid premium-metric-grid">
            <div className="account-stat account-stat-xp">
              <span className="account-stat-icon">
                <DashboardMetricIcon kind="xp" />
              </span>
              <span>
                <small>XP/h</small>
                <strong>{format.format(account.xpPerHour)}</strong>
              </span>
            </div>
            <div className="account-stat account-stat-gold">
              <span className="account-stat-icon">
                <GameGoldIcon />
              </span>
              <span>
                <small>Gold/h</small>
                <strong>{format.format(account.goldPerHour)}</strong>
              </span>
            </div>
            <div className="account-stat account-stat-item">
              <span className="account-combat-icon">
                {potion ? <ItemAsset item={potion} catalog={catalog} compact /> : '—'}
              </span>
              <span>
                <small>{account.potionName ?? 'Potions'}</small>
                <strong>{format.format(account.potions)}</strong>
              </span>
            </div>
            <div className="account-stat account-stat-item">
              <span className="account-combat-icon">
                {ball ? <ItemAsset item={ball} catalog={catalog} compact /> : '—'}
              </span>
              <span>
                <small>{account.ballName ?? 'Balls'}</small>
                <strong>{format.format(account.balls)}</strong>
              </span>
            </div>
          </div>
        </div>
      </div>
      <footer className="premium-account-footer">
        <div className="premium-account-hunt">
          <img src={huntIcon} className="premium-account-hunt-icon" alt="" aria-hidden="true" />
          {account.hunt ? (
            <>
              <span>Hunt:</span>
              <b>{account.hunt}</b>
              {huntElapsed && (
                <span
                  className="premium-account-hunt-timer"
                  aria-label={`Tempo desde a seleção da hunt: ${huntElapsed}`}
                  title={`Tempo desde a seleção confirmada da hunt: ${huntElapsed}`}
                >
                  <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8.5" r="5.6" /><path d="M8 5.2v3.5l2.1 1.2M6 1.5h4" /></svg>
                  <span>{huntElapsed}</span>
                </span>
              )}
            </>
          ) : (
            'Aguardando uma hunt ativa'
          )}
        </div>
        <div className="premium-account-balances" aria-label="Saldos da conta">
          <span
            className="premium-account-balance premium-account-balance-gold"
            role="group"
            aria-label={`Gold: ${format.format(account.gold)}`}
          >
            <span className="premium-account-balance-icon" aria-hidden="true">
              <GameGoldIcon />
            </span>
            <strong>{format.format(account.gold)}</strong>
          </span>
          <span
            className="premium-account-balance premium-account-balance-gems"
            role="group"
            aria-label={`Gemas: ${format.format(account.gems)}`}
          >
            <span className="premium-account-balance-icon" aria-hidden="true">
              <GameGemIcon />
            </span>
            <strong>{format.format(account.gems)}</strong>
          </span>
        </div>
      </footer>
    </Card>
  )
}

function IntegrationPanel({ diagnostic }: { diagnostic: IntegrationDiagnostic }) {
  const steps = [
    ['Brave encontrado', diagnostic.braveFound],
    ['Perfil isolado criado', diagnostic.profileCreated],
    ['Processo Brave iniciado', diagnostic.braveStarted],
    ['Endpoint CDP disponível', diagnostic.cdpEndpointAvailable],
    ['Browser WS URL obtida', diagnostic.browserWsUrlObtained],
    ['Browser WS conectado', diagnostic.browserWsConnected],
    ['Target GAME_URL criado/reutilizado', diagnostic.targetFound],
    ['Target ID encontrado', diagnostic.targetIdFound],
    ['Sessão da aba criada', diagnostic.sessionCreated],
    ['Page.enable', diagnostic.pageEnabled],
    ['Network habilitado', diagnostic.networkEnabled],
    ['GAME_URL aberta diretamente', diagnostic.pageNavigateSent],
    ['Navegação GAME_URL', diagnostic.gameUrlNavigated],
    [`Socket pokeidle.io (${diagnostic.websocketCount})`, diagnostic.pokeidleSocketDetected],
    ['hello detectado', diagnostic.helloDetected],
    ['welcome recebido', diagnostic.welcomeReceived],
    ['URL WS capturada', diagnostic.wsUrlCaptured],
    ['Material efêmero da sessão', diagnostic.sessionMaterialCaptured],
    ['Rust WS conectado', diagnostic.rustWsConnected],
    ['hello Rust enviado', diagnostic.rustHelloSent],
    ['welcome Rust recebido', diagnostic.rustWelcomeReceived],
    ['Brave encerrado', diagnostic.browserClosed],
    ['Modo Background', diagnostic.backgroundActive],
  ] as const
  return (
    <Card className="integration-panel">
      <p className="eyebrow">INTEGRAÇÃO REAL</p>
      <h2>{diagnostic.state}</h2>
      <p>{diagnostic.message}</p>
      <p className="integration-port">
        Modo:{' '}
        {diagnostic.sessionMode === 'backgroundBootstrap' ? 'Background bootstrap' : 'Interativo'}
      </p>
      <p className="integration-port">Porta CDP: {diagnostic.cdpPort ?? '—'}</p>
      {diagnostic.browserProduct && (
        <p className="integration-port">Browser: {diagnostic.browserProduct}</p>
      )}
      {diagnostic.cdpEndpointAttempts > 0 && (
        <p className="integration-port">Tentativas do endpoint: {diagnostic.cdpEndpointAttempts}</p>
      )}
      {diagnostic.finalUrl && <p className="integration-port">URL final: {diagnostic.finalUrl}</p>}
      {diagnostic.cdpEndpointLastError && (
        <p className="muted">Último erro CDP: {diagnostic.cdpEndpointLastError}</p>
      )}
      {diagnostic.automaticReloadUsed && (
        <p className="muted">Reload automático controlado: usado</p>
      )}
      <div className="integration-steps">
        {steps.map(([label, complete]) => (
          <span key={label} className={complete ? 'complete' : ''}>
            {complete ? '✓' : '○'} {label}
          </span>
        ))}
      </div>
      {diagnostic.nick && <p className="muted">Nick confirmado: {diagnostic.nick}</p>}
    </Card>
  )
}

function Dashboard({
  diagnostic,
  onStartReal,
  addAccountBusy,
  addAccountError,
  onRetryAddAccount,
  hydrationComplete,
}: {
  diagnostic: IntegrationDiagnostic | null
  onStartReal: () => void
  addAccountBusy: boolean
  addAccountError: string | null
  onRetryAddAccount: (() => void) | null
  hydrationComplete: boolean
}) {
  const accounts = useAppStore(selectVisibleAccounts)
  const { settings, addMockAccounts } = useAppStore()
  const catalog = useGameItemCatalog()
  const [nowMs, setNowMs] = useState(() => Date.now())
  const hasHuntTimer = accounts.some((account) => account.huntStartedAtMs != null)
  useEffect(() => {
    if (!hasHuntTimer) return
    setNowMs(Date.now())
    const timer = window.setInterval(() => setNowMs(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [hasHuntTimer])
  // Connection lifecycle is account data, never a route. A diagnostic may
  // occupy this initial empty state only before any account has been created.
  if (startupDebug && !settings.mockMode && hydrationComplete && !accounts.length && diagnostic)
    return (
      <main className="page dashboard empty-page">
        <PageHeader title="Dashboard" />
        {addAccountError && <AddAccountFeedback message={addAccountError} onRetry={onRetryAddAccount ?? undefined} />}
        <IntegrationPanel diagnostic={diagnostic} />
        <Button onClick={onStartReal} disabled={addAccountBusy || Boolean(diagnostic.accountId)}>
          + Adicionar conta
        </Button>
      </main>
    )
  if (!accounts.length && !hydrationComplete)
    return (
      <main className="page dashboard empty-page">
        <PageHeader title="Dashboard" />
        {addAccountError && <AddAccountFeedback message={addAccountError} onRetry={onRetryAddAccount ?? undefined} />}
        <p className="muted">Carregando contas persistidas...</p>
      </main>
    )
  if (!accounts.length)
    return (
      <main className="page dashboard empty-page">
        <PageHeader title="Dashboard" />
        {addAccountError && <AddAccountFeedback message={addAccountError} onRetry={onRetryAddAccount ?? undefined} />}
        <EmptyState onAdd={settings.mockMode ? addMockAccounts : onStartReal} disabled={addAccountBusy && !settings.mockMode} />
      </main>
    )
  const online = accounts.filter((account) => account.status === 'online')
  const totals: Array<{ label: string; value: number | string; kind: DashboardMetricKind }> = [
    {
      label: 'XP/h total',
      value: online.reduce((sum, account) => sum + account.xpPerHour, 0),
      kind: 'xp',
    },
    {
      label: 'Gold/h total',
      value: online.reduce((sum, account) => sum + account.goldPerHour, 0),
      kind: 'gold',
    },
    { label: 'Contas online', value: `${online.length}/4`, kind: 'accounts' },
    {
      label: 'Gold total',
      value: accounts.reduce((sum, account) => sum + account.gold, 0),
      kind: 'gold',
    },
    {
      label: 'Gemas totais',
      value: accounts.reduce((sum, account) => sum + account.gems, 0),
      kind: 'gems',
    },
  ]
  return (
    <main className="page dashboard">
      <PageHeader
        title="Dashboard"
        actions={
          <div className="dashboard-actions">
            <Button
              onClick={settings.mockMode ? addMockAccounts : onStartReal}
              disabled={addAccountBusy || (!settings.mockMode && accounts.length >= 4)}
            >
              + Adicionar conta
            </Button>
            {!settings.mockMode && accounts.length >= 4 && (
              <small className="muted">Limite de 4 contas atingido</small>
            )}
          </div>
        }
      />
      {addAccountError && <AddAccountFeedback message={addAccountError} onRetry={onRetryAddAccount ?? undefined} />}
      <div className="totals totals-five">
        {totals.map((total) => (
          <Card key={total.label} className={`premium-total premium-total-${total.kind}`}>
            <span className="premium-total-icon">
              {total.kind === 'gold' ? (
                <GameGoldIcon />
              ) : total.kind === 'gems' ? (
                <GameGemIcon />
              ) : (
                <DashboardMetricIcon kind={total.kind} />
              )}
            </span>
            <span>
              <small>{total.label}</small>
              <strong>
                {typeof total.value === 'number' ? format.format(total.value) : total.value}
              </strong>
            </span>
          </Card>
        ))}
      </div>
      <div className="account-grid">
        {accounts.map((account) => (
          <AccountCard key={account.id} account={account} catalog={catalog} nowMs={nowMs} />
        ))}
      </div>
    </main>
  )
}

type DetailIconKind =
  | 'account'
  | 'pokemon'
  | 'farm'
  | 'metrics'
  | 'automations'
  | 'events'
  | 'level'
  | 'gold'
  | 'gem'
  | 'map'
  | 'hunt'
  | 'capture'
  | 'drop'

function DetailIcon({ kind }: { kind: DetailIconKind }) {
  const common = { viewBox: '0 0 24 24', 'aria-hidden': true as const }
  if (kind === 'account') return <svg {...common}><circle cx="12" cy="8" r="3.5" /><path d="M4.5 20c.8-4.1 3.3-6.2 7.5-6.2s6.7 2.1 7.5 6.2" /></svg>
  if (kind === 'pokemon' || kind === 'capture') return <svg {...common}><circle cx="12" cy="12" r="8.5" /><path d="M3.5 12h17M12 9.2a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6Z" /></svg>
  if (kind === 'farm') return <svg {...common}><path d="m4 19 5.5-6 3.5 3 4.8-7L21 19M4 19h17M17.8 9H21v3.2" /></svg>
  if (kind === 'metrics') return <svg {...common}><path d="M4 20V12M10 20V7M16 20V10M22 20V3" /></svg>
  if (kind === 'automations') return <svg {...common}><circle cx="12" cy="12" r="3" /><path d="M19.3 15.1 21 16l-1.6 2.8-1.8-.6a7.8 7.8 0 0 1-2.3 1.3L15 21h-3l-.4-1.5a7.8 7.8 0 0 1-2.3-1.3l-1.8.6L6 16l1.7-.9a8.2 8.2 0 0 1 0-2.2L6 12l1.5-2.8 1.8.6a7.8 7.8 0 0 1 2.3-1.3L12 7h3l.3 1.5a7.8 7.8 0 0 1 2.3 1.3l1.8-.6L21 12l-1.7.9a8.2 8.2 0 0 1 0 2.2Z" /></svg>
  if (kind === 'events') return <svg {...common}><path d="M6 4h12v16H6zM9 8h6M9 12h6M9 16h4M8 2v4M16 2v4" /></svg>
  if (kind === 'level') return <svg {...common}><path d="m12 3 2.5 5.2 5.5.8-4 3.9.9 5.5-4.9-2.7-4.9 2.7.9-5.5-4-3.9 5.5-.8L12 3Z" /></svg>
  if (kind === 'gold') return <svg {...common}><circle cx="12" cy="12" r="8.5" /><path d="M14.6 8.5c-.6-.5-1.4-.8-2.5-.8-1.4 0-2.5.8-2.5 2 0 1.4 1.1 1.8 2.5 2.1 1.5.3 2.6.7 2.6 2.1 0 1.2-1 2.1-2.7 2.1-1.2 0-2.3-.4-3.1-1.1M12 6.4v11.2" /></svg>
  if (kind === 'gem') return <svg {...common}><path d="m12 3 7 5-7 13L5 8l7-5ZM5 8h14M12 3v18" /></svg>
  if (kind === 'map') return <svg {...common}><path d="m4 6 5-2 6 2 5-2v14l-5 2-6-2-5 2V6ZM9 4v14M15 6v14" /></svg>
  if (kind === 'drop') return <svg {...common}><path d="M12 3c3.3 4 5 6.6 5 9.3a5 5 0 0 1-10 0C7 9.6 8.7 7 12 3Z" /></svg>
  return <svg {...common}><path d="M4 18 20 6M7 5h6v6M17 19h-6v-6" /></svg>
}

function DetailCardHeading({ icon, children }: { icon: DetailIconKind; children: ReactNode }) {
  return (
    <header className="detail-card-heading">
      <span className="detail-heading-icon">
        <DetailIcon kind={icon} />
      </span>
      <h2>{children}</h2>
    </header>
  )
}

function DetailMeter({
  label,
  detail,
  value,
  tone = 'cyan',
}: {
  label: string
  detail: string
  value: number
  tone?: 'cyan' | 'green'
}) {
  return (
    <div className={`detail-meter detail-meter-${tone}`}>
      <div>
        <span>{label}</span>
        <b>{detail}</b>
      </div>
      <div className="progress">
        <i style={{ width: `${Math.min(100, value)}%` }} />
      </div>
    </div>
  )
}

function AccountDetail({ account, onBrowserAction, browserActionBusy }: {
  account: AccountView
  onBrowserAction: (accountId: string, command: BrowserAccountCommand) => Promise<void>
  browserActionBusy: boolean
}) {
  const back = useAppStore((state) => state.closeAccountDetail)
  const isDesktop = '__TAURI_INTERNALS__' in window
  const depotRevision = account.domainRevisions?.depot
  const hasWildPokemon = Boolean(account.wildPokemon)
  useEffect(() => {
    if (!isDesktop || !hasWildPokemon || depotRevision === undefined) return
    void ensureAccountDomain(account.id, 'depot').catch((error: unknown) => {
      console.warn('[account-detail] Não foi possível carregar o depósito.', error)
    })
  }, [account.id, depotRevision, hasWildPokemon, isDesktop])
  const catalog = useGameItemCatalog()
  const [transferError, setTransferError] = useState<string | null>(null)
  const [confirmRemoval, setConfirmRemoval] = useState(false)
  const transferring = account.mode === 'transitioning'
  const reconnecting = transferring && account.runtime === 'reconnecting'
  const transfer = (
    command: 'open_account_browser' | 'return_account_to_background' | 'login_account' | 'reconnect_browser_control',
  ) => {
    setTransferError(null)
    if (command !== 'return_account_to_background') {
      void onBrowserAction(account.id, command).catch((error: unknown) =>
        setTransferError(String(error)),
      )
      return
    }
    void invoke(command, { accountId: account.id }).catch((error: unknown) =>
      setTransferError(String(error)),
    )
  }
  const remove = () => {
    setTransferError(null)
    void invoke('remove_account', { accountId: account.id, confirmed: true })
      .then(() => back())
      .catch((error: unknown) => setTransferError(String(error)))
  }
  const hp = account.maxHp ? Math.round((account.hp / account.maxHp) * 100) : 0
  const levelStart = account.xpLevel
  const levelXp = Math.max(0, account.xp - levelStart)
  const levelRequired = Math.max(0, account.xpNext - levelStart)
  const xp = levelRequired ? Math.round((levelXp / levelRequired) * 100) : 0
  const activePokemon = activePokemonAsDepot(account)
  const wildTypes = account.wildPokemon
    ? (account.depot.find((pokemon) => pokemon.name === account.wildPokemon?.name)?.types ?? [])
    : []
  const killsPerHour = account.onlineSeconds
    ? Math.round((account.kills * 3600) / account.onlineSeconds)
    : 0
  const capturesPerHour = account.onlineSeconds
    ? Math.round((account.captures * 3600) / account.onlineSeconds)
    : 0
  const vipLabel =
    account.vip === true
      ? `Ativo${account.vipExpiresAt ? ` até ${new Date(account.vipExpiresAt).toLocaleString('pt-BR')}` : ''}`
      : account.vip === false
        ? 'Não ativo'
        : account.vipDataAvailable
          ? 'Dados recebidos'
          : '—'
  const sessionEvents = [
    ...(account.wildPokemon
      ? [
          {
            icon: 'farm' as const,
            description: `Em combate com ${account.wildPokemon.name}`,
            time: 'Agora',
          },
        ]
      : []),
    ...(account.goldGained > 0
      ? [
          {
            icon: 'gold' as const,
            description: `+${format.format(account.goldGained)} Gold obtido`,
            time: 'Sessão',
          },
        ]
      : []),
    ...(account.xpGained > 0
      ? [
          {
            icon: 'metrics' as const,
            description: `+${format.format(account.xpGained)} XP obtido`,
            time: 'Sessão',
          },
        ]
      : []),
    ...account.drops
      .slice(0, 3)
      .map((drop) => ({ icon: 'drop' as const, description: drop, time: 'Sessão' })),
  ]
  return (
    <main className="account-detail-route">
      <div
        className="page-scroll"
        tabIndex={0}
        role="region"
        aria-label={`Detalhes de ${account.nick}`}
      >
        <section className="account-detail-page">
          <PageHeader
            title={account.nick}
            actions={
              <div className="account-detail-header-actions">
                <div className="detail-statuses">
                  <Badge tone={toneFor(account.status)}>{statusFor(account.status)}</Badge>
                  <span className={`detail-mode detail-mode-${account.mode}`}>
                    {account.mode === 'browser' && <img src={browserModeIcon} alt="" aria-hidden="true" className="account-mode-mark" />}
                    {account.mode === 'background' && <img src={backgroundModeIcon} alt="" aria-hidden="true" className="account-mode-mark" />}
                    {account.mode === 'transitioning' && <span className="account-mode-mark" aria-hidden="true">↻</span>}
                    {modeFor(account.mode)}
                  </span>
                  <span className="detail-uptime">◷ {time(account.onlineSeconds)}</span>
                </div>
                <div className="detail-actions">
                  <Button className="detail-back" onClick={back}>
                    ← Voltar ao Dashboard
                  </Button>
                  {account.mode === 'background' && (
                    <Button className="detail-open-browser" disabled={browserActionBusy} onClick={() => transfer('open_account_browser')}>
                      Abrir navegador
                    </Button>
                  )}
                  {account.mode === 'browser' && account.status === 'error' && (
                    <Button
                      className="detail-open-browser"
                      disabled={browserActionBusy}
                      onClick={() => transfer('reconnect_browser_control')}
                    >
                      Reconectar navegador
                    </Button>
                  )}
                  {account.mode === 'browser' && account.status !== 'error' && (
                    <Button className="detail-return-background" onClick={() => transfer('return_account_to_background')}>
                      Voltar ao background
                    </Button>
                  )}
                  {account.status === 'login_required' && (
                    <Button disabled={browserActionBusy} onClick={() => transfer('login_account')}>Fazer login</Button>
                  )}
                  {transferring && (
                    <Button disabled>{reconnecting ? 'Reconectando...' : 'Transferindo...'}</Button>
                  )}
                  {!confirmRemoval ? (
                    <Button className="danger-button" onClick={() => setConfirmRemoval(true)}>
                      Remover conta
                    </Button>
                  ) : (
                    <span className="removal-confirmation">
                      <small>Remover do Manager? O perfil local será mantido.</small>
                      <Button onClick={() => setConfirmRemoval(false)}>Cancelar</Button>
                      <Button className="danger-button" onClick={remove}>
                        Confirmar remoção
                      </Button>
                    </span>
                  )}
                </div>
                {transferError && <p className="error-message">{transferError}</p>}
              </div>
            }
          />
          <div className="detail-premium-grid">
            <Card className="detail-premium-card detail-account-card">
              <DetailCardHeading icon="account">Informações da conta</DetailCardHeading>
              <div className="detail-account-identity">
                <span className="detail-account-avatar" style={{ background: account.color }}>
                  {account.nick[0]}
                </span>
                <div>
                  <strong>{account.nick}</strong>
                  {account.vip && (
                    <span className="detail-account-vip-badge">
                      <img src={vipIcon} alt="" aria-hidden="true" />
                      VIP
                    </span>
                  )}
                </div>
              </div>
              <div className="detail-account-stats">
                <div className="detail-account-stat">
                  <AccountInfoIcon kind="level" />
                  <span>
                    <small>Nível</small>
                    <strong>{account.level}</strong>
                  </span>
                </div>
                <div className="detail-account-stat detail-account-stat-gold">
                  <AccountInfoIcon kind="gold" />
                  <span>
                    <small>Gold</small>
                    <strong>{format.format(account.gold)}</strong>
                  </span>
                </div>
                <div className="detail-account-stat detail-account-stat-gems">
                  <AccountInfoIcon kind="gems" />
                  <span>
                    <small>Gemas</small>
                    <strong>{format.format(account.gems)}</strong>
                  </span>
                </div>
                <div className="detail-account-stat detail-account-stat-diamonds">
                  <AccountInfoIcon kind="diamonds" />
                  <span>
                    <small>Diamantes</small>
                    <strong>{format.format(account.diamonds)}</strong>
                  </span>
                </div>
              </div>
              <dl className="detail-info-list detail-account-info-list">
                <div>
                  <dt>
                    <AccountInfoIcon kind="vip" />
                    VIP
                  </dt>
                  <dd className={account.vip ? 'detail-vip-active' : ''}>{vipLabel}</dd>
                </div>
                <div>
                  <dt>
                    <AccountInfoIcon kind="map" />
                    Mapa
                  </dt>
                  <dd>{account.map}</dd>
                </div>
                <div>
                  <dt>
                    <AccountInfoIcon kind="hunt" />
                    Hunt
                  </dt>
                  <dd>{account.hunt ?? '—'}</dd>
                </div>
              </dl>
            </Card>
            <Card className="detail-premium-card detail-pokemon-card">
              <DetailCardHeading icon="pokemon">Pokémon ativo</DetailCardHeading>
              <div className="detail-pokemon-overview">
                <div className="detail-pokemon-art">
                  {activePokemon ? (
                    <PokemonAsset pokemon={activePokemon} catalog={catalog} size={144} />
                  ) : (
                    <span>?</span>
                  )}
                </div>
                <div>
                  <h3>{account.pokemon}</h3>
                  <p>Nv. {account.level}</p>
                  <div className="detail-type-badges">
                    {account.types.map((type) => (
                      <b key={type}>{type}</b>
                    ))}
                  </div>
                </div>
              </div>
              <DetailMeter
                label="HP"
                detail={`${format.format(account.hp)} / ${format.format(account.maxHp)} · ${hp}%`}
                value={hp}
                tone="green"
              />
              <DetailMeter
                label="XP"
                detail={`${format.format(levelXp)} / ${format.format(levelRequired)} · ${xp}%`}
                value={xp}
              />
              <div className="detail-stat-chips">
                <span>Potência P{account.power || '—'}</span>
                <span>Qualidade {account.quality.toFixed(3)}</span>
              </div>
            </Card>
            <Card className="detail-premium-card detail-farm-card">
              <DetailCardHeading icon="farm">Selvagem e farm</DetailCardHeading>
              {account.wildPokemon ? (
                <>
                  <div className="detail-wild-heading">
                    <div>
                      <h3>{account.wildPokemon.name}</h3>
                      <p>Nv. {account.wildPokemon.level}</p>
                    </div>
                    <div className="detail-type-badges">
                      {wildTypes.map((type) => (
                        <b key={type}>{type}</b>
                      ))}
                    </div>
                  </div>
                  <DetailMeter
                    label="HP"
                    detail={`${format.format(account.wildPokemon.hp)} / ${format.format(account.wildPokemon.maxHp)}`}
                    value={Math.round((account.wildPokemon.hp / account.wildPokemon.maxHp) * 100)}
                    tone="green"
                  />
                </>
              ) : (
                <p className="detail-empty-state">Nenhum encontro ativo.</p>
              )}
              <dl className="detail-farm-stats">
                <div>
                  <dt>Kills</dt>
                  <dd>{format.format(account.kills)}</dd>
                </div>
                <div>
                  <dt>Capturas</dt>
                  <dd>{format.format(account.captures)}</dd>
                </div>
                <div>
                  <dt>XP obtido</dt>
                  <dd>{format.format(account.xpGained)}</dd>
                </div>
                <div>
                  <dt>Gold obtido</dt>
                  <dd>{format.format(account.goldGained)}</dd>
                </div>
              </dl>
              <div className="detail-drop-list">
                {account.drops.length ? (
                  account.drops.slice(0, 3).map((drop) => (
                    <span key={drop}>
                      <DetailIcon kind="drop" />
                      {drop}
                    </span>
                  ))
                ) : (
                  <span>Sem drops na sessão.</span>
                )}
              </div>
            </Card>
            <Card className="detail-premium-card detail-metrics-card">
              <DetailCardHeading icon="metrics">Métricas de rendimento</DetailCardHeading>
              <div className="detail-metrics-grid">
                {[
                  ['XP/h', format.format(account.xpPerHour), 'xp'],
                  ['Gold/h', format.format(account.goldPerHour), 'gold'],
                  ['Kills/h', format.format(killsPerHour), 'farm'],
                  ['Capturas/h', format.format(capturesPerHour), 'capture'],
                  ['Potions/h', format.format(account.potionsPerHour), 'gem'],
                  ['Balls/h', format.format(account.ballsPerHour), 'pokemon'],
                ].map(([label, value, icon]) => (
                  <div key={label}>
                    <span>
                      <DetailIcon kind={icon as DetailIconKind} />
                    </span>
                    <p>
                      {label}
                      <b>{value}</b>
                    </p>
                  </div>
                ))}
              </div>
            </Card>
            <Card className="detail-premium-card detail-automations-card">
              <DetailCardHeading icon="automations">Automações</DetailCardHeading>
              <div className="detail-automation-list">
                {(Object.keys(automationLabels) as AutomationKind[]).map((kind) => (
                  <div key={kind}>
                    <span className={`detail-automation-icon detail-automation-icon-${kind}`}>
                      <AutomationIcon kind={kind} />
                    </span>
                    <span>{automationLabels[kind]}</span>
                    <Badge tone={account.automations[kind] ? 'positive' : 'neutral'}>
                      {account.automations[kind] ? 'Ativa' : 'Desativada'}
                    </Badge>
                  </div>
                ))}
              </div>
            </Card>
            <Card className="detail-premium-card detail-events-card">
              <DetailCardHeading icon="events">Últimos eventos</DetailCardHeading>
              <div className="detail-event-list">
                {sessionEvents.map((event, index) => (
                  <div key={`${event.description}-${index}`}>
                    <span className={`detail-event-icon detail-event-icon-${event.icon}`}>
                      <DetailIcon kind={event.icon} />
                    </span>
                    <p>{event.description}</p>
                    <time>{event.time}</time>
                  </div>
                ))}
                {!sessionEvents.length && (
                  <p className="detail-empty-state">Sem atividade recente sincronizada.</p>
                )}
              </div>
            </Card>
          </div>
        </section>
      </div>
    </main>
  )
}
function AccountSelector() {
  const accounts = useAppStore(selectVisibleAccounts),
    selectedIds = useAppStore((state) => state.ui.selectedIds),
    selectAccounts = useAppStore((state) => state.selectAccounts)
  return (
    <section className="automation-account-bar" aria-label="Contas selecionadas">
      <div className="automation-account-bar-heading">
        <span>CONTAS</span>
        <Badge tone="warning">{selectedIds.length} selecionada(s)</Badge>
      </div>
      <div className="account-selector">
        <Button
          className={`account-select-all ${selectedIds.length === accounts.length ? 'selected' : ''}`}
          onClick={() => selectAccounts(accounts.map((account) => account.id))}
        >
          Selecionar todas
        </Button>
        {accounts.map((account) => {
          const selected = selectedIds.includes(account.id)
          return (
            <Button
              key={account.id}
              className={`account-selection-chip ${selected ? 'selected' : ''}`}
              onClick={() =>
                selectAccounts(
                  selected
                    ? selectedIds.filter((id) => id !== account.id)
                    : [...selectedIds, account.id],
                )
              }
            >
              <span className={`account-selection-dot account-selection-dot-${account.status}`} />
              {account.nick}
            </Button>
          )
        })}
      </div>
    </section>
  )
}
function HuntControl() {
  const accounts = useAppStore(selectVisibleAccounts)
  const selectedIds = useAppStore((state) => state.ui.selectedIds)
  const settings = useAppStore((state) => state.settings)
  const [slug, setSlug] = useState('')
  const [huntQuery, setHuntQuery] = useState('')
  const [huntSearchOpen, setHuntSearchOpen] = useState(false)
  const [results, setResults] = useState<Record<string, string>>({})
  const [visualNow, setVisualNow] = useState(() => Date.now())
  const selectedAccounts = accounts.filter((account) => selectedIds.includes(account.id))
  const combatLocked = (account: AccountView) =>
    account.combatLockUntil !== undefined &&
    account.serverOffsetMs !== undefined &&
    visualNow + account.serverOffsetMs < account.combatLockUntil
  const lockedAccounts = selectedAccounts.filter(combatLocked)
  const transportUnavailable = selectedAccounts.some(
    (account) => account.commandTransportAvailable === false,
  )
  const pendingAccounts = selectedAccounts.filter((account) => account.pendingNavigation)
  const currentHunt = selectedAccounts.find((account) => account.hunt)?.hunt ?? null
  const currentHuntTypes = currentHunt
    ? [
        ...new Set(
          selectedAccounts.flatMap(
            (account) =>
              account.depot.find(
                (pokemon) => pokemon.name.toLocaleLowerCase() === currentHunt.toLocaleLowerCase(),
              )?.types ?? [],
          ),
        ),
      ]
    : []
  const remainingMs = lockedAccounts.reduce(
    (maximum, account) =>
      Math.max(
        maximum,
        (account.combatLockUntil ?? 0) - (visualNow + (account.serverOffsetMs ?? 0)),
      ),
    0,
  )
  const hunts = useMemo(() => {
    const options = new Map<string, { slug: string; name: string }>()
    for (const account of selectedAccounts) {
      for (const hunt of account.huntOptions ?? []) {
        options.set(hunt.slug, { slug: hunt.slug, name: hunt.name })
      }
    }
    return [...options.values()]
  }, [selectedAccounts])
  const selectedHunt = hunts.find((hunt) => hunt.slug === slug)
  const filteredHunts = hunts.filter((hunt) =>
    hunt.name.toLocaleLowerCase().includes(huntQuery.trim().toLocaleLowerCase()),
  )
  useEffect(() => {
    if (!hunts.some((hunt) => hunt.slug === slug)) setSlug(hunts[0]?.slug ?? '')
  }, [hunts, slug])
  useEffect(() => {
    if (!huntSearchOpen) setHuntQuery(selectedHunt?.name ?? '')
  }, [huntSearchOpen, selectedHunt?.name])
  useEffect(() => {
    if (!lockedAccounts.length) return
    const timer = window.setInterval(() => setVisualNow(Date.now()), 200)
    return () => window.clearInterval(timer)
  }, [lockedAccounts.length, remainingMs])
  const requesting = selectedAccounts.some((account) => account.pendingHunt === slug)
  const start = async () => {
    if (!slug || !selectedAccounts.length || settings.mockMode) return
    setResults(
      Object.fromEntries(selectedAccounts.map((account) => [account.id, 'Solicitando...'])),
    )
    await Promise.all(
      selectedAccounts.map(async (account) => {
        try {
          await invoke('select_hunt', { accountId: account.id, slug })
          setResults((current) => ({
            ...current,
            [account.id]: 'Aguardando confirmação do jogo...',
          }))
        } catch (error) {
          setResults((current) => ({ ...current, [account.id]: `Indisponível: ${String(error)}` }))
        }
      }),
    )
  }
  return (
    <Card className="hunt-control">
      <header className="automation-panel-heading">
        <p>CONTROLE DE HUNT</p>
      </header>
      <div className="hunt-control-grid">
        <div className="hunt-current" aria-label="Hunt atual">
          <small>Hunt atual</small>
          <div className="hunt-current-main">
            <strong>{currentHunt ?? 'Nenhuma Hunt ativa'}</strong>
            {!!currentHuntTypes.length && (
              <span className="hunt-type-badges">
                {currentHuntTypes.map((type) => (
                  <b key={type}>{type}</b>
                ))}
              </span>
            )}
          </div>
        </div>
        <div className="hunt-search">
          <label htmlFor="hunt-search">Selecionar Hunt</label>
          <div className="hunt-search-control">
            <input
              id="hunt-search"
              aria-label="Buscar Hunt"
              aria-autocomplete="list"
              aria-controls="hunt-search-results"
              aria-expanded={huntSearchOpen && !!hunts.length}
              autoComplete="off"
              disabled={!hunts.length}
              placeholder={hunts.length ? 'Digite o nome da Hunt' : 'Nenhuma Hunt disponível'}
              role="combobox"
              value={huntQuery}
              onBlur={() => setHuntSearchOpen(false)}
              onChange={(event) => {
                setHuntQuery(event.target.value)
                setHuntSearchOpen(true)
              }}
              onFocus={() => setHuntSearchOpen(true)}
              onKeyDown={(event) => {
                if (event.key === 'Escape') {
                  setHuntSearchOpen(false)
                  return
                }
                if (event.key === 'Enter' && filteredHunts.length === 1) {
                  event.preventDefault()
                  const [hunt] = filteredHunts
                  setSlug(hunt.slug)
                  setHuntQuery(hunt.name)
                  setHuntSearchOpen(false)
                }
              }}
            />
            {huntSearchOpen && !!hunts.length && (
              <div id="hunt-search-results" className="hunt-search-results" role="listbox">
                {filteredHunts.map((hunt) => (
                  <button
                    key={hunt.slug}
                    aria-selected={hunt.slug === slug}
                    role="option"
                    type="button"
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => {
                      setSlug(hunt.slug)
                      setHuntQuery(hunt.name)
                      setHuntSearchOpen(false)
                    }}
                  >
                    {hunt.name}
                  </button>
                ))}
                {!filteredHunts.length && <p>Nenhuma Hunt encontrada.</p>}
              </div>
            )}
          </div>
        </div>
        <div className="hunt-actions">
          <Button
            onClick={() => void start()}
            disabled={
              !slug ||
              !selectedAccounts.length ||
              requesting ||
              settings.mockMode ||
              transportUnavailable
            }
          >
            {transportUnavailable
              ? 'Aguardando transporte...'
              : requesting
                ? 'Solicitando...'
                : lockedAccounts.length
                  ? 'Agendar Hunt'
                  : 'Iniciar Hunt'}
          </Button>
          <Button
            onClick={() =>
              void Promise.all(
                selectedAccounts.map((account) => invoke('go_center', { accountId: account.id })),
              )
            }
            disabled={!selectedAccounts.length || settings.mockMode || transportUnavailable}
          >
            Voltar ao Center
          </Button>
        </div>
      </div>
      <div className="hunt-status-row">
        {!!Object.keys(results).length && (
          <div className="hunt-results" aria-live="polite">
            {selectedAccounts.map((account) => (
              <small key={account.id}>
                {account.nick}:{' '}
                {account.pendingHunt ? 'Solicitando...' : (results[account.id] ?? '—')}
              </small>
            ))}
          </div>
        )}
        {!!pendingAccounts.length && (
          <div className="combat-lock" aria-live="polite">
            {pendingAccounts.map((account) => {
              const destination =
                account.pendingNavigation?.kind === 'center'
                  ? 'Pokémon Center'
                  : (account.pendingNavigation?.slug ?? 'Hunt')
              const seconds = Math.max(
                0,
                ((account.combatLockUntil ?? 0) - (visualNow + (account.serverOffsetMs ?? 0))) /
                  1000,
              )
              return (
                <p key={account.id}>
                  Aguardando: {destination} · {seconds.toFixed(1)}s{' '}
                  <button
                    onClick={() => void invoke('cancel_navigation', { accountId: account.id })}
                  >
                    Cancelar
                  </button>
                </p>
              )
            })}
          </div>
        )}
        {!!lockedAccounts.length && !pendingAccounts.length && (
          <p className="combat-lock" aria-live="polite">
            Em combate: escolha a próxima Hunt ou o Center; o Manager enviará ao liberar.
          </p>
        )}
        {transportUnavailable && (
          <p className="muted">
            Conta em transferência, offline ou com transporte ainda indisponível.
          </p>
        )}
      </div>
    </Card>
  )
}
function Automations() {
  const accounts = useAppStore(selectVisibleAccounts),
    { selectedIds } = useAppStore((state) => state.ui),
    { applyAutomation, setPotionThreshold, settings } = useAppStore()
  const realDomains = useAppStore((state) => state.real.domains)
  const selectedRealAccounts = accounts.filter(
    (account) => selectedIds.includes(account.id) && account.domainRevisions,
  )
  const domainLoadKey = selectedRealAccounts
    .map((account) =>
      [
        account.id,
        account.domainRevisions?.depot,
        account.domainRevisions?.inventory,
        account.domainRevisions?.hunt_options,
      ].join(':'),
    )
    .join('|')
  const isDesktop = '__TAURI_INTERNALS__' in window
  const [domainLoadError, setDomainLoadError] = useState<string | null>(null)
  const [domainRetry, setDomainRetry] = useState(0)
  useEffect(() => {
    let current = true
    setDomainLoadError(null)
    if (!isDesktop || settings.mockMode || !domainLoadKey) {
      return () => {
        current = false
      }
    }
    const accountIds = domainLoadKey.split('|').map((item) => item.split(':')[0])
    void Promise.all(
      accountIds.flatMap((accountId) => [
        ensureAccountDomain(accountId, 'depot'),
        ensureAccountDomain(accountId, 'inventory'),
        ensureAccountDomain(accountId, 'hunt_options'),
      ]),
    ).catch((error: unknown) => {
      if (current) setDomainLoadError(String(error))
      console.warn('[automations] Falha ao carregar domínios.', error)
    })
    return () => {
      current = false
    }
  }, [domainLoadKey, domainRetry, isDesktop, settings.mockMode])
  const domainsLoading = selectedRealAccounts.some((account) => {
    const revisions = account.domainRevisions
    const cache = realDomains?.[account.id]
    return (
      !revisions ||
      cache?.depot?.revision !== revisions.depot ||
      cache?.inventory?.revision !== revisions.inventory ||
      cache?.hunt_options?.revision !== revisions.hunt_options
    )
  })
  const [draftThreshold, setDraftThreshold] = useState<number | null>(null)
  const selectedThreshold =
    accounts.find((account) => selectedIds.includes(account.id))?.potionThreshold ?? 40
  const transportUnavailable = accounts
    .filter((account) => selectedIds.includes(account.id))
    .some((account) => account.commandTransportAvailable === false)
  const automationError = accounts.find(
    (account) => selectedIds.includes(account.id) && account.automationError,
  )?.automationError
  useEffect(() => setDraftThreshold(null), [selectedThreshold, selectedIds])
  return (
    <main className="automation-route">
      <div className="page-scroll" tabIndex={0} role="region" aria-label="Conteúdo de Automações">
        <section className="automation-page">
          <PageHeader title="Automações" />
          <AccountSelector />
          <HuntControl />
          <CombatItemPreferences />
          {domainsLoading && !domainLoadError && (
            <p className="muted" role="status">Carregando dados das contas…</p>
          )}
          {domainLoadError && (
            <p className="muted" role="alert">
              Não foi possível carregar os dados das contas.{' '}
              <button type="button" onClick={() => setDomainRetry((retry) => retry + 1)}>
                Tentar novamente
              </button>
            </p>
          )}
          {automationError && (
            <p className="muted" role="status">
              Última automação: {automationError}
            </p>
          )}
          <section className="automation-section" aria-label="Automações da conta">
            <p className="automation-section-label">AUTOMAÇÕES DA CONTA</p>
            <div className="automation-grid">
              {(Object.keys(automationLabels) as AutomationKind[]).map((kind) => {
                const enabled =
                  selectedIds.length > 0 &&
                  selectedIds.every(
                    (id) => accounts.find((account) => account.id === id)?.automations[kind],
                  )
                const threshold = draftThreshold ?? selectedThreshold
                const autoBuyKind =
                  kind === 'autoBuyPotion' ? 'item' : kind === 'autoBuyBall' ? 'ball' : undefined
                return (
                  <Card
                    key={kind}
                    className={`automation-card ${autoBuyKind ? 'automation-card-buy' : ''}`}
                  >
                    <div className="automation-card-heading">
                      <span className={`automation-card-icon automation-card-icon-${kind}`}>
                        <AutomationIcon kind={kind} />
                      </span>
                      <div className="automation-card-copy">
                        <h3>{automationLabels[kind]}</h3>
                        <p>{automationDescriptions[kind]}</p>
                      </div>
                      <Switch
                        label={`Ativar ${automationLabels[kind]}`}
                        checked={enabled}
                        disabled={transportUnavailable}
                        onCheckedChange={(checked) => {
                          if (settings.mockMode) return applyAutomation(kind, checked)
                          if (kind === 'ballUntilCapture' || kind === 'ballContinuous') {
                            const mode = checked
                              ? kind === 'ballUntilCapture'
                                ? 'until_capture'
                                : 'continuous'
                              : 'off'
                            for (const accountId of selectedIds)
                              void invoke('set_capture_mode', { accountId, mode })
                            return
                          }
                          if (kind === 'autoBuyPotion' || kind === 'autoBuyBall') {
                            for (const accountId of selectedIds) {
                              void invoke('set_auto_buy_enabled', {
                                accountId,
                                kind: kind === 'autoBuyPotion' ? 'item' : 'ball',
                                enabled: checked,
                              })
                            }
                            return
                          }
                          const nativeFields: Partial<Record<AutomationKind, string>> = {
                            autoPotion: 'autoPotion',
                            autoRevive: 'autoRevive',
                            autoVendaLoot: 'autoVendaLoot',
                            returnToHunt: 'autoVoltarHunt',
                          }
                          const field = nativeFields[kind]
                          if (field) {
                            for (const accountId of selectedIds) {
                              void invoke('set_native_automation', {
                                accountId,
                                field,
                                enabled: checked,
                              })
                            }
                          }
                        }}
                      />
                    </div>
                    {autoBuyKind && (
                      <AutoBuyConfig
                        accounts={accounts}
                        selectedIds={selectedIds}
                        kind={autoBuyKind}
                        disabled={transportUnavailable}
                      />
                    )}
                    {kind === 'autoPotion' && (
                      <label className="threshold">
                        Usar quando HP ≤ {threshold}%
                        <input
                          aria-label="Limiar de HP"
                          type="range"
                          min="10"
                          max="100"
                          step="10"
                          value={threshold}
                          disabled={transportUnavailable}
                          onChange={(event) => {
                            const next = Number(event.target.value)
                            setDraftThreshold(next)
                            if (settings.mockMode) setPotionThreshold(next)
                          }}
                          onPointerUp={() => {
                            if (settings.mockMode) return
                            for (const accountId of selectedIds) {
                              void invoke('set_native_hp_threshold', {
                                accountId,
                                percent: threshold,
                              })
                            }
                          }}
                          onKeyUp={(event) => {
                            if (
                              settings.mockMode ||
                              !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)
                            )
                              return
                            for (const accountId of selectedIds) {
                              void invoke('set_native_hp_threshold', {
                                accountId,
                                percent: threshold,
                              })
                            }
                          }}
                        />
                      </label>
                    )}
                  </Card>
                )
              })}
            </div>
          </section>
        </section>
      </div>
    </main>
  )
}

const potionCatalog = [
  { id: 200, name: 'Small Potion' },
  { id: 201, name: 'Great Potion' },
  { id: 202, name: 'Ultra Potion' },
  { id: 203, name: 'Hyper Potion' },
  { id: 204, name: 'Ultimate Potion' },
  { id: 70070, name: 'Golden Potion' },
]
const ballCatalog = [
  { id: 1, name: 'Poké Ball' },
  { id: 2, name: 'Great Ball' },
  { id: 3, name: 'Super Ball' },
  { id: 4, name: 'Ultra Ball' },
]
type CombatPreference = 'potionIds' | 'ballIds'

function sameOrder(values: number[][]) {
  if (!values.length) return []
  return values.every(
    (value) =>
      value.length === values[0].length && value.every((id, index) => id === values[0][index]),
  )
    ? values[0]
    : undefined
}
function CombatItemPreferences() {
  const catalog = useGameItemCatalog()
  const accounts = useAppStore(selectVisibleAccounts)
  const { selectedIds } = useAppStore((state) => state.ui)
  const { settings } = useAppStore()
  const selected = accounts.filter((account) => selectedIds.includes(account.id))
  const configured = (field: CombatPreference) =>
    selected.map(
      (account) =>
        (field === 'potionIds' ? account.potionIds : account.ballIds)
          ?.map(Number)
          .filter(Number.isFinite) ?? [],
    )
  const potionIds = sameOrder(configured('potionIds'))
  const ballIds = sameOrder(configured('ballIds'))
  const apply = (field: CombatPreference, ids: number[]) => {
    if (settings.mockMode) return
    for (const account of selected)
      void invoke('set_native_automation_ids', { accountId: account.id, field, ids })
  }
  const toggle = (field: CombatPreference, id: number) => {
    const current = (field === 'potionIds' ? potionIds : ballIds) ?? configured(field)[0] ?? []
    apply(field, current.includes(id) ? current.filter((value) => value !== id) : [...current, id])
  }
  const combinedStock = (id: number, kind: 'potion' | 'ball') =>
    selected.reduce((total, account) => {
      const stock =
        kind === 'potion'
          ? (account.inventory.find((item) => item.id === String(id))?.quantity ?? 0)
          : (account.inventory.find((item) => item.id === `ball-${id}`)?.quantity ?? 0)
      return total + stock
    }, 0)
  const preferenceCard = (
    title: string,
    field: CombatPreference,
    options: typeof potionCatalog,
    ids: number[] | undefined,
    kind: 'potion' | 'ball',
  ) => (
    <Card className="combat-preference-card">
      <h3 className="combat-selection-title">SELEÇÃO DE {title.toUpperCase()}</h3>
      {ids === undefined && selected.length > 1 && (
        <p className="muted">
          Seleções diferentes entre contas. Escolha uma opção para aplicar a todas.
        </p>
      )}
      <div className={`combat-item-options combat-item-options-${kind}`}>
        {options.map((item) => {
          const checked = ids?.includes(item.id) ?? false
          return (
            <div className={checked ? 'combat-item selected' : 'combat-item'} key={item.id}>
              <button
                type="button"
                className="combat-item-choice"
                role="checkbox"
                aria-label={`Usar ${item.name}`}
                aria-checked={checked}
                disabled={!selected.length || settings.mockMode}
                onClick={() => toggle(field, item.id)}
              >
                <span className="item-icon compact-item-icon">
                  <ItemAsset
                    catalog={catalog}
                    compact
                    item={{
                      id: kind === 'ball' ? `ball-${item.id}` : String(item.id),
                      assetKey: kind === 'ball' ? `ball-${item.id}` : String(item.id),
                      name: item.name,
                      quantity: 0,
                      category: kind === 'ball' ? 'balls' : 'potions',
                    }}
                  />
                </span>
                <span className="combat-item-name">{item.name}</span>
                <strong className="combat-item-quantity">
                  {format.format(combinedStock(item.id, kind))}
                </strong>
              </button>
            </div>
          )
        })}
      </div>
      {ids?.length === 0 && (
        <p className="muted">Nenhuma {kind === 'potion' ? 'Potion' : 'Ball'} selecionada.</p>
      )}
    </Card>
  )
  return (
    <section className="combat-preferences" aria-label="Poções e Balls configuradas">
      {preferenceCard('Poções', 'potionIds', potionCatalog, potionIds, 'potion')}
      {preferenceCard('Balls', 'ballIds', ballCatalog, ballIds, 'ball')}
    </section>
  )
}

function AutoBuyConfig({
  accounts,
  selectedIds,
  kind,
  disabled,
}: {
  accounts: AccountView[]
  selectedIds: string[]
  kind: 'item' | 'ball'
  disabled: boolean
}) {
  const catalog = useGameItemCatalog()
  const selected = accounts.filter((account) => selectedIds.includes(account.id))
  const selectionKey = selected
    .map((account) => account.id)
    .sort()
    .join(',')
  const [drafts, setDrafts] = useState<
    Record<
      string,
      {
        value: string
        version: number
        status: 'editing' | 'saving' | 'pending'
        expectedValue?: number
        accountIds?: string[]
      }
    >
  >({})
  const [feedback, setFeedback] = useState<
    Record<string, { tone: 'success' | 'warning' | 'error'; text: string }>
  >({})
  const idsFor = (account: AccountView) =>
    ((kind === 'item' ? account.potionIds : account.ballIds) ?? [])
      .map(Number)
      .filter(Number.isFinite)
  const itemIds = [...new Set(selected.flatMap(idsFor))]
  const draftKey = (itemId: number, field: 'minimum' | 'quantity') =>
    `${selectionKey}|${kind}|${itemId}|${field}`

  useEffect(() => {
    const prefix = `${selectionKey}|`
    setDrafts((current) =>
      Object.fromEntries(Object.entries(current).filter(([key]) => key.startsWith(prefix))),
    )
    setFeedback((current) =>
      Object.fromEntries(Object.entries(current).filter(([key]) => key.startsWith(prefix))),
    )
  }, [selectionKey])

  useEffect(() => {
    const confirmed = Object.entries(drafts)
      .filter(([key, draft]) => {
        if (!key.startsWith(`${selectionKey}|`) || draft.status !== 'pending') return false
        const [, itemIdText, field] = key.slice(`${selectionKey}|`.length).split('|')
        const itemId = Number(itemIdText)
        return (draft.accountIds ?? []).every((accountId) => {
          const account = selected.find((candidate) => candidate.id === accountId)
          const rule = account?.autoBuyRules?.find(
            (candidate) => candidate.kind === kind && candidate.itemId === itemId,
          )
          const value = field === 'minimum' ? (rule?.minimum ?? 500) : (rule?.quantity ?? 1000)
          return value === draft.expectedValue
        })
      })
      .map(([key]) => key)
    if (confirmed.length) {
      setDrafts((current) => {
        const next = { ...current }
        for (const key of confirmed) delete next[key]
        return next
      })
    }
  }, [drafts, kind, selected, selectionKey])

  const labelFor = (itemId: number) =>
    kind === 'item'
      ? (potionCatalog.find((item) => item.id === itemId)?.name ?? `Potion #${itemId}`)
      : (ballCatalog.find((item) => item.id === itemId)?.name ?? `Ball #${itemId}`)
  if (!itemIds.length)
    return (
      <p className="muted">
        Selecione ao menos uma {kind === 'item' ? 'Potion' : 'Ball'} nas Preferências de combate
        para configurar a reposição.
      </p>
    )
  return (
    <div className="auto-buy-config multi-item-auto-buy">
      {itemIds.map((itemId) => {
        const compatible = selected.filter((account) => idsFor(account).includes(itemId))
        const incompatible = selected.filter((account) => !idsFor(account).includes(itemId))
        const ruleFor = (account: AccountView) =>
          account.autoBuyRules?.find((rule) => rule.kind === kind && rule.itemId === itemId)
        const valuesFor = (field: 'minimum' | 'quantity') =>
          compatible.map((account) => {
            const rule = ruleFor(account)
            return field === 'minimum' ? (rule?.minimum ?? 500) : (rule?.quantity ?? 1000)
          })
        const commonValue = (values: number[]) =>
          values.length > 0 && values.every((value) => value === values[0]) ? values[0] : undefined
        const minimumValue = commonValue(valuesFor('minimum'))
        const quantityValue = commonValue(valuesFor('quantity'))
        const minimumKey = draftKey(itemId, 'minimum')
        const quantityKey = draftKey(itemId, 'quantity')
        const minimumDraft = drafts[minimumKey]
        const quantityDraft = drafts[quantityKey]
        const itemFeedback = feedback[`${selectionKey}|${kind}|${itemId}|feedback`]

        const edit = (field: 'minimum' | 'quantity', value: string) => {
          const key = draftKey(itemId, field)
          setDrafts((current) => ({
            ...current,
            [key]: {
              value,
              version: (current[key]?.version ?? 0) + 1,
              status: 'editing',
            },
          }))
          setFeedback((current) => {
            const next = { ...current }
            delete next[`${selectionKey}|${kind}|${itemId}|feedback`]
            return next
          })
        }

        const commit = async (field: 'minimum' | 'quantity') => {
          const key = draftKey(itemId, field)
          const draft = drafts[key]
          if (!draft || draft.status !== 'editing') return
          const parsed = Number(draft.value)
          const value =
            field === 'minimum'
              ? Math.max(0, Number.isFinite(parsed) ? parsed : 0)
              : Math.min(9999, Math.max(1, Number.isFinite(parsed) ? parsed : 1))
          const version = draft.version
          const accountIds = compatible.map((account) => account.id)
          if (!accountIds.length) return
          setDrafts((current) =>
            current[key]?.version === version
              ? { ...current, [key]: { ...current[key], value: String(value), status: 'saving' } }
              : current,
          )
          const results = await Promise.all(
            compatible.map(async (account) => {
              const currentRule = ruleFor(account)
              try {
                await invoke('update_auto_buy_rule', {
                  accountId: account.id,
                  kind,
                  itemId,
                  minimum: field === 'minimum' ? value : (currentRule?.minimum ?? 500),
                  quantity: field === 'quantity' ? value : (currentRule?.quantity ?? 1000),
                })
                return { account, ok: true as const }
              } catch (error) {
                return { account, ok: false as const, error }
              }
            }),
          )
          const succeeded = results.filter((result) => result.ok)
          const failed = results.filter((result) => !result.ok)
          const feedbackKey = `${selectionKey}|${kind}|${itemId}|feedback`
          if (!succeeded.length) {
            setDrafts((current) => {
              const next = { ...current }
              if (next[key]?.version === version) delete next[key]
              return next
            })
            setFeedback((current) => ({
              ...current,
              [feedbackKey]: {
                tone: 'error',
                text: `Falha ao atualizar: ${failed.map((result) => result.account.nick).join(', ')}.`,
              },
            }))
            return
          }
          if (failed.length) {
            setDrafts((current) => {
              const next = { ...current }
              if (next[key]?.version === version) delete next[key]
              return next
            })
            setFeedback((current) => ({
              ...current,
              [feedbackKey]: {
                tone: 'warning',
                text: `Atualizado em ${succeeded.length}; falhou em ${failed.map((result) => result.account.nick).join(', ')}.`,
              },
            }))
            return
          }
          setDrafts((current) =>
            current[key]?.version === version
              ? {
                  ...current,
                  [key]: { ...current[key], status: 'pending', expectedValue: value, accountIds },
                }
              : current,
          )
          setFeedback((current) => ({
            ...current,
            [feedbackKey]: {
              tone: 'success',
              text: `Atualização enviada para ${succeeded.length} conta(s); aguardando sincronização.`,
            },
          }))
        }

        const fieldInput = (field: 'minimum' | 'quantity') => {
          const draft = field === 'minimum' ? minimumDraft : quantityDraft
          const consensus = field === 'minimum' ? minimumValue : quantityValue
          const mixed = consensus === undefined
          const label = field === 'minimum' ? 'Estoque mínimo' : 'Quantidade'
          return (
            <>
              {mixed && (
                <span className="auto-buy-mixed" role="status" aria-label={`${label}: MIXED`}>
                  Vários valores
                </span>
              )}
              <input
                aria-label={`${label} ${labelFor(itemId)}${mixed ? ' (MIXED, vários valores)' : ''}`}
                type="number"
                min={field === 'minimum' ? '0' : '1'}
                {...(field === 'quantity' ? { max: '9999' } : {})}
                disabled={disabled || compatible.length === 0}
                value={draft?.value ?? (mixed ? '' : String(consensus))}
                placeholder={mixed ? 'Vários valores' : undefined}
                onChange={(event) => edit(field, event.currentTarget.value)}
                onBlur={() => void commit(field)}
              />
            </>
          )
        }

        return (
          <div className="auto-buy-item" key={itemId}>
            <strong className="auto-buy-item-name">
              <span className="compact-item-icon">
                <ItemAsset
                  catalog={catalog}
                  compact
                  item={{
                    id: kind === 'ball' ? `ball-${itemId}` : String(itemId),
                    assetKey: kind === 'ball' ? `ball-${itemId}` : String(itemId),
                    name: labelFor(itemId),
                    quantity: 0,
                    category: kind === 'ball' ? 'balls' : 'potions',
                  }}
                />
              </span>
              {labelFor(itemId)}
            </strong>
            <label>
              Estoque mínimo
              {fieldInput('minimum')}
            </label>
            <label>
              Quantidade por compra
              {fieldInput('quantity')}
            </label>
            {incompatible.length > 0 && (
              <p className="auto-buy-incompatible" role="status">
                Não configurado em: {incompatible.map((account) => account.nick).join(', ')}.
                Alterações serão aplicadas somente às contas compatíveis.
              </p>
            )}
            {itemFeedback && (
              <p
                className={`auto-buy-feedback auto-buy-feedback-${itemFeedback.tone}`}
                role="status"
              >
                {itemFeedback.text}
              </p>
            )}
          </div>
        )
      })}
    </div>
  )
}

function Inventory() {
  const catalog = useGameItemCatalog()
  const accounts = useAppStore(selectVisibleAccounts)
  const realDomains = useAppStore((state) => state.real.domains)
  const settings = useAppStore((state) => state.settings)
  const isDesktop = '__TAURI_INTERNALS__' in window
  const inventoryTab = useAppStore((state) => state.ui.inventoryTab)
  const setInventoryTab = useAppStore((state) => state.setInventoryTab)
  const [accountId, setAccountId] = useState('all')
  const [query, setQuery] = useState('')
  const [category, setCategory] = useState<InventoryCategory | 'all'>('all')
  const [type, setType] = useState('all')
  const [minIv, setMinIv] = useState<number | null>(null)
  const [order, setOrder] = useState<PokemonSort>('level')
  const [confirmDepot, setConfirmDepot] = useState(false)
  const [confirmPokemon, setConfirmPokemon] = useState<
    ReturnType<typeof depotPokemon>[number] | null
  >(null)
  const [saleError, setSaleError] = useState<string | null>(null)
  const [pokemonGridViewport, setPokemonGridViewport] = useState({
    width: 900,
    height: 700,
    scrollTop: 0,
  })
  const pokemonGridScrollRef = useRef<HTMLDivElement>(null)
  const selectedAccounts =
    accountId === 'all' ? accounts : accounts.filter((account) => account.id === accountId)
  const domainLoadKey = selectedAccounts
    .filter((account) => account.domainRevisions)
    .map(
      (account) =>
        `${account.id}:${account.domainRevisions?.depot}:${account.domainRevisions?.inventory}`,
    )
    .join('|')
  const [domainLoadError, setDomainLoadError] = useState('')
  useEffect(() => {
    if (!isDesktop || settings.mockMode || !domainLoadKey) return
    setDomainLoadError('')
    const accountIds = domainLoadKey.split('|').map((item) => item.split(':')[0])
    void Promise.all(
      accountIds.flatMap((id) => [
        ensureAccountDomain(id, 'depot'),
        ensureAccountDomain(id, 'inventory'),
      ]),
    ).catch((error: unknown) => setDomainLoadError(String(error)))
  }, [domainLoadKey, isDesktop, settings.mockMode])
  const missingDomains = selectedAccounts.some((account) => {
    const revisions = account.domainRevisions
    if (!revisions) return false
    const cache = realDomains?.[account.id]
    return (
      cache?.depot?.revision !== revisions.depot ||
      cache?.inventory?.revision !== revisions.inventory
    )
  })
  const allItems = useMemo(
    () =>
      aggregateItems(accounts, accountId).map((item) => ItemAssetResolver.describe(item, catalog)),
    [accounts, accountId, catalog],
  )
  const items = useMemo(() => filterItems(allItems, query, category), [allItems, category, query])
  const allPokemon = useMemo(() => depotPokemon(accounts, accountId), [accounts, accountId])
  const types = useMemo(
    () => [...new Set(depotPokemon(accounts).flatMap((entry) => entry.types))].sort(),
    [accounts],
  )
  const pokemon = useMemo(
    () => filterPokemon(allPokemon, query, type, minIv, order),
    [allPokemon, minIv, order, query, type],
  )
  const pokemonWindow = pokemonGridWindow(
    pokemon.length,
    pokemonGridViewport.width,
    pokemonGridViewport.height,
    pokemonGridViewport.scrollTop,
  )
  const visiblePokemon = pokemon.slice(pokemonWindow.startIndex, pokemonWindow.endIndex)
  useEffect(() => {
    const element = pokemonGridScrollRef.current
    if (element) element.scrollTop = 0
    setPokemonGridViewport((current) =>
      current.scrollTop ? { ...current, scrollTop: 0 } : current,
    )
  }, [accountId, minIv, order, query, type])
  useEffect(() => {
    const element = pokemonGridScrollRef.current
    if (!element) return
    const updateViewport = () =>
      setPokemonGridViewport((current) => ({
        width: element.clientWidth || current.width,
        height: element.clientHeight || current.height,
        scrollTop: element.scrollTop,
      }))
    updateViewport()
    if (typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(updateViewport)
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  const lockSettings = selectedAccounts[0]?.depotLocks
  return (
    <main className="page inventory-page">
      {domainLoadError && (
        <p className="inventory-domain-status" role="status">
          Não foi possível carregar os dados: {domainLoadError}
        </p>
      )}
      {!domainLoadError && missingDomains && (
        <p className="inventory-domain-status" role="status">
          Carregando dados da conta…
        </p>
      )}
      <PageHeader
        title="Inventários"
        actions={
          <label className="inventory-account">
            Conta
            <select
              aria-label="Conta do inventário"
              value={accountId}
              onChange={(event) => setAccountId(event.target.value)}
            >
              <option value="all">Todas as contas</option>
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.nick}
                </option>
              ))}
            </select>
          </label>
        }
      />
      <div className="tabs inventory-tabs">
        <button
          className={inventoryTab === 'items' ? 'active' : ''}
          onClick={() => setInventoryTab('items')}
        >
          Itens
        </button>
        <button
          className={inventoryTab === 'pokemon' ? 'active' : ''}
          onClick={() => setInventoryTab('pokemon')}
        >
          Pokémon
        </button>
      </div>
      {inventoryTab === 'items' ? (
        <section className="inventory-workspace" aria-label="Bag de itens">
          <div className="inventory-toolbar">
            <input
              aria-label="Buscar item"
              placeholder="Buscar item"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <div className="item-filters" aria-label="Categorias de item">
              {(
                [
                  ['all', 'Todos'],
                  ['potions', 'Potions'],
                  ['balls', 'Balls'],
                  ['stones', 'Stones'],
                  ['other', 'Outros'],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  className={category === value ? 'active' : ''}
                  onClick={() => setCategory(value)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
          <div className="item-grid-scroll">
            <div className="item-grid">
              {items.map((item) => (
                <article
                  className="item-card"
                  key={item.id}
                  title={item.owners.map((owner) => `${owner.nick}  ${owner.quantity}`).join('\n')}
                >
                  <span className={`item-icon item-${item.category}`}>
                    <ItemAsset item={item} catalog={catalog} />
                  </span>
                  <b>{item.name}</b>
                  <strong>{format.format(item.quantity)}</strong>
                  {accountId === 'all' && item.owners.length > 1 && (
                    <small>{item.owners.length} contas</small>
                  )}
                </article>
              ))}
              {!items.length && <p className="muted">Nenhum item encontrado com esses filtros.</p>}
            </div>
          </div>
        </section>
      ) : (
        <section className="inventory-workspace pokemon-workspace" aria-label="Depot de Pokémon">
          <div className="pokemon-controls">
            <input
              aria-label="Buscar Pokémon"
              placeholder="Buscar Pokémon"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <label>
              Tipo
              <select
                aria-label="Tipo de Pokémon"
                value={type}
                onChange={(event) => setType(event.target.value)}
              >
                <option value="all">Todos os tipos</option>
                {types.map((entry) => (
                  <option key={entry}>{entry}</option>
                ))}
              </select>
            </label>
            <label>
              IV mínimo
              <input
                aria-label="IV mínimo"
                type="number"
                min="0"
                step="1"
                placeholder="Qualquer"
                value={minIv ?? ''}
                onChange={(event) =>
                  setMinIv(
                    event.target.value === '' ? null : Math.max(0, Number(event.target.value)),
                  )
                }
              />
            </label>
            <label>
              Ordenar por
              <select
                aria-label="Ordenar Pokémon"
                value={order}
                onChange={(event) => setOrder(event.target.value as PokemonSort)}
              >
                <option value="level">Nível</option>
                <option value="quality">Qualidade</option>
                <option value="power">Potência</option>
                <option value="note">Maior Nota Calculadora</option>
                <option value="iv">Maior IV Total</option>
                <option value="type">Tipo</option>
                <option value="recent">Recentes</option>
              </select>
            </label>
          </div>
          <div className="protocol-locks">
            <span>Venda confirmada</span>
            <label>
              <input
                type="checkbox"
                checked={lockSettings?.autoLockShiny ?? false}
                readOnly
                disabled
              />{' '}
              Auto Lock Shiny
            </label>
            <label>
              <input
                type="checkbox"
                checked={lockSettings?.autoLockNota9 ?? false}
                readOnly
                disabled
              />{' '}
              Auto Lock Nota ≥ {lockSettings?.autoLockNotaMin ?? '—'}
            </label>
            <label>
              <input
                type="checkbox"
                checked={lockSettings?.autoLockP5 ?? false}
                readOnly
                disabled
              />{' '}
              Auto Lock P5
            </label>
            <Button onClick={() => setConfirmDepot(true)} disabled={!pokemon.length}>
              Vender todo o Depot
            </Button>
          </div>
          <div
            className="pokemon-grid-scroll"
            ref={pokemonGridScrollRef}
            onScroll={(event) => {
              const scrollTop = event.currentTarget.scrollTop
              setPokemonGridViewport((current) =>
                Math.floor(current.scrollTop / POKEMON_GRID_ROW_HEIGHT) ===
                Math.floor(scrollTop / POKEMON_GRID_ROW_HEIGHT)
                  ? current
                  : { ...current, scrollTop },
              )
            }}
          >
            {pokemon.length ? (
              <div
                className="pokemon-grid-virtual-space"
                style={{ height: pokemonWindow.totalHeight }}
                data-virtual-start={pokemonWindow.startIndex}
                data-virtual-end={pokemonWindow.endIndex}
              >
                <div
                  className="pokemon-grid pokemon-grid-virtual-window"
                  style={{
                    top: pokemonWindow.top,
                    gridTemplateColumns: `repeat(${pokemonWindow.columns}, minmax(0, 1fr))`,
                  }}
                >
                  {visiblePokemon.map((entry) => (
                    <article className="depot-card" key={entry.id}>
                      <div className="depot-owner">
                        <span style={{ background: entry.color }} />
                        {entry.nick}
                      </div>
                      <span
                        className="lock-state"
                        aria-label={entry.locked ? 'Protegido' : 'Não protegido'}
                      >
                        {entry.locked && <span aria-hidden="true">♢</span>}
                      </span>
                      <div className="pokemon-sprite">
                        <PokemonAsset pokemon={entry} catalog={catalog} />
                      </div>
                      <h3>
                        {entry.shiny && <><span className="market-shiny-mark" aria-hidden="true">✨</span>{' '}</>}
                        {entry.name}
                      </h3>
                      <p>
                        Nv. {entry.level} · {entry.types.join(' / ')}
                      </p>
                      <dl>
                        <dt>Potência</dt>
                        <dd>{entry.power ? `P${entry.power}` : '—'}</dd>
                        <dt>IV</dt>
                        <dd>{entry.ivTotal ?? '—'}</dd>
                        <dt>Q</dt>
                        <dd>{entry.quality?.toFixed(3) ?? '—'}</dd>
                        <dt>Nota</dt>
                        <dd>{entry.note?.toFixed(3) ?? '—'}</dd>
                      </dl>
                      <div className="depot-flags">
                        {entry.shiny && <span>Shiny</span>}
                        {entry.p5 && <span>P5</span>}
                      </div>
                      <Button
                        disabled={
                          entry.locked ||
                          accounts.find((account) => account.id === entry.accountId)
                            ?.commandTransportAvailable === false
                        }
                        title={entry.locked ? 'Pokémon protegido' : undefined}
                        onClick={() => {
                          setSaleError(null)
                          setConfirmPokemon(entry)
                        }}
                      >
                        Vender
                      </Button>
                    </article>
                  ))}
                </div>
              </div>
            ) : (
              <div className="pokemon-grid">
                <p className="muted">Nenhum Pokémon encontrado com esses filtros.</p>
              </div>
            )}
          </div>
          {confirmPokemon && (
            <div className="confirm-layer" role="dialog" aria-label="Confirmar venda de Pokémon">
              <Card>
                <h2>
                  Vender {confirmPokemon.name} Nv. {confirmPokemon.level}?
                </h2>
                <p>
                  Esta ação é irreversível. O Pokémon só será removido da tela após confirmação e
                  reconciliação do jogo.
                </p>
                {saleError && <p className="muted">{saleError}</p>}
                <div className="dialog-actions">
                  <Button onClick={() => setConfirmPokemon(null)}>Cancelar</Button>
                  <Button
                    onClick={() =>
                      void invoke('sell_pokemon', {
                        accountId: confirmPokemon.accountId,
                        pokemonId: Number(confirmPokemon.id),
                        confirmed: true,
                      })
                        .then(() => setConfirmPokemon(null))
                        .catch((error) => setSaleError(String(error)))
                    }
                  >
                    Vender
                  </Button>
                </div>
              </Card>
            </div>
          )}
          {confirmDepot && (
            <div className="confirm-layer" role="dialog" aria-label="Confirmar venda do depot">
              <Card>
                <h2>Vender todos os Pokémon não protegidos do Depot?</h2>
                <p>
                  Pokémon protegidos/locked serão preservados pelo servidor. Esta ação é
                  irreversível.
                </p>
                <p className="muted">
                  {accountId === 'all'
                    ? `${selectedAccounts.length} contas serão afetadas.`
                    : `Conta: ${selectedAccounts[0]?.nick ?? ''}`}
                </p>
                {saleError && <p className="muted">{saleError}</p>}
                <div className="dialog-actions">
                  <Button onClick={() => setConfirmDepot(false)}>Cancelar</Button>
                  <Button
                    onClick={() =>
                      void Promise.all(
                        selectedAccounts.map((account) =>
                          invoke('sell_all_pokemon', { accountId: account.id, confirmed: true }),
                        ),
                      )
                        .then(() => setConfirmDepot(false))
                        .catch((error) => setSaleError(String(error)))
                    }
                  >
                    Vender todos
                  </Button>
                </div>
              </Card>
            </div>
          )}
        </section>
      )}
    </main>
  )
}
function Market() {
  const accounts = useAppStore(selectVisibleAccounts)
  const market = useMarketStore((state) => state.snapshot)
  const setMarketSnapshot = useMarketStore((state) => state.setSnapshot)
  const baseCatalog = useGameItemCatalog()
  const catalog = useMemo(
    () => mergeMarketCatalog(baseCatalog, market?.itemMetadata ?? []),
    [baseCatalog, market?.itemMetadata],
  )
  const [currency, setCurrency] = useState<'all' | MarketCurrency>('all')
  const [query, setQuery] = useState('')
  const [itemSearch, setItemSearch] = useState('')
  const [itemPickerOpen, setItemPickerOpen] = useState(false)
  const [itemPickerScrollTop, setItemPickerScrollTop] = useState(0)
  const [marketRowsScrollTop, setMarketRowsScrollTop] = useState(0)
  const [purchaseHistoryScrollTop, setPurchaseHistoryScrollTop] = useState(0)
  const [ruleError, setRuleError] = useState('')
  const [readerError, setReaderError] = useState('')
  const [editingRuleId, setEditingRuleId] = useState<string | null>(null)
  const [purchaseAccountFilter, setPurchaseAccountFilter] = useState('all')
  const ruleEditorRef = useRef<HTMLElement>(null)
  const [draft, setDraft] = useState({
    accountId: '',
    itemId: '',
    currency: 'gold' as MarketCurrency,
    maxPrice: '',
    budget: '',
    minimumBalance: '',
  })
  const refresh = () =>
    void invoke<MarketSnapshot>('market_snapshot')
      .then(setMarketSnapshot)
      .catch(() => undefined)
  useEffect(() => {
    if (!draft.accountId && accounts[0])
      setDraft((current) => ({ ...current, accountId: accounts[0].id }))
  }, [accounts, draft.accountId])
  const rows = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return (market?.summaries ?? [])
      .filter(
        (summary) =>
          currency === 'all' ||
          (currency === 'gold' ? summary.minGold !== null : summary.minOrb !== null),
      )
      .map((summary) => ({ summary, item: marketItemForId(summary.itemId, catalog) }))
      .filter(({ item }) => !needle || item.name.toLowerCase().includes(needle))
      .sort(
        (left, right) =>
          (left.summary.minGold ?? Number.MAX_SAFE_INTEGER) -
          (right.summary.minGold ?? Number.MAX_SAFE_INTEGER),
      )
  }, [catalog, currency, market?.summaries, query])
  const marketItemIds = useMemo(
    () => new Set((market?.summaries ?? []).map((summary) => summary.itemId)),
    [market?.summaries],
  )
  const matchingItems = useMemo(() => {
    const needle = itemSearch.trim().toLowerCase()
    // The live `market.itens` response is the authoritative rule list. The
    // catalog decorates it, but cannot hide a just-released Market item.
    return [...marketItemIds]
      .map((itemId) => marketItemForId(itemId, catalog))
      .filter((item) => !needle || item.name.toLowerCase().includes(needle))
      .sort((left, right) => left.name.localeCompare(right.name))
  }, [catalog, itemSearch, marketItemIds])
  const marketRowHeight = 42
  const marketRowStart = Math.max(0, Math.floor(marketRowsScrollTop / marketRowHeight) - 4)
  const visibleMarketRows = rows.slice(marketRowStart, marketRowStart + 18)
  const pickerRowHeight = 42
  const pickerRowStart = Math.max(0, Math.floor(itemPickerScrollTop / pickerRowHeight) - 4)
  const visiblePickerItems = matchingItems.slice(pickerRowStart, pickerRowStart + 16)
  const visiblePurchases = useMemo(
    () =>
      (market?.purchases ?? []).filter(
        (purchase) =>
          purchaseAccountFilter === 'all' || purchase.accountId === purchaseAccountFilter,
      ),
    [market?.purchases, purchaseAccountFilter],
  )
  const historyRowStart = Math.max(
    0,
    Math.floor(purchaseHistoryScrollTop / MARKET_HISTORY_ROW_HEIGHT) - 3,
  )
  const renderedPurchases = visiblePurchases.slice(historyRowStart, historyRowStart + 12)
  const selectedDraftItem = draft.itemId
    ? marketItemForId(Number(draft.itemId), catalog)
    : undefined
  const saveRule = (rule: MarketSniperRule) =>
    void invoke('save_market_sniper_rule', { rule })
      .then(refresh)
      .catch((error) => setRuleError(String(error)))
  const resetRuleEditor = () => {
    setEditingRuleId(null)
    setItemSearch('')
    setItemPickerOpen(false)
    setRuleError('')
    setDraft({
      accountId: accounts[0]?.id ?? '',
      itemId: '',
      currency: 'gold',
      maxPrice: '',
      budget: '',
      minimumBalance: '',
    })
  }
  const editRule = (rule: MarketSniperRule) => {
    setEditingRuleId(rule.id)
    setItemSearch('')
    setItemPickerOpen(false)
    setRuleError('')
    setDraft({
      accountId: rule.accountId,
      itemId: String(rule.itemId),
      currency: rule.currency,
      maxPrice: String(rule.maxPrice),
      budget: rule.budget > 0 ? String(rule.budget) : '',
      minimumBalance: rule.minimumBalance ? String(rule.minimumBalance) : '',
    })
    ruleEditorRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' })
  }
  const createRule = () => {
    const itemId = Number(draft.itemId)
    const maxPrice = Number(draft.maxPrice)
    const budget = draft.budget.trim() ? Number(draft.budget) : 0
    const minimumBalance = draft.minimumBalance.trim() ? Number(draft.minimumBalance) : 0
    if (
      !draft.accountId ||
      !Number.isInteger(itemId) ||
      itemId <= 0 ||
      !Number.isSafeInteger(maxPrice) ||
      maxPrice <= 0
    ) {
      setRuleError('Escolha a conta e o item, depois informe o preço máximo por unidade.')
      return
    }
    if (
      !Number.isSafeInteger(budget) ||
      budget < 0 ||
      !Number.isSafeInteger(minimumBalance) ||
      minimumBalance < 0
    ) {
      setRuleError(
        'Os limites de gastos e saldo mínimo devem ser valores inteiros iguais ou maiores que zero.',
      )
      return
    }
    const existing = (market?.rules ?? []).find((rule) => rule.id === editingRuleId)
    setRuleError('')
    void invoke('save_market_sniper_rule', {
      rule: {
        id: editingRuleId ?? crypto.randomUUID(),
        accountId: draft.accountId,
        targetType: 'item',
        enabled: existing?.enabled ?? true,
        itemId,
        currency: draft.currency,
        maxPrice,
        quantity: existing?.quantity ?? 0,
        budget,
        minimumBalance,
        spent: existing?.spent ?? 0,
        purchasedQuantity: existing?.purchasedQuantity ?? 0,
      },
    })
      .then(() => {
        refresh()
        resetRuleEditor()
      })
      .catch((error) => setRuleError(String(error)))
  }
  const eligibleReaderAccounts = accounts.filter(
    (account) => account.status === 'online' && account.commandTransportAvailable,
  )
  const setReader = (accountId: string) => {
    setReaderError('')
    void invoke('set_market_reader', { accountId: accountId || null })
      .then(refresh)
      .catch((error) => setReaderError(String(error)))
  }
  const topItemSales = useMemo(
    () =>
      (market?.topItemSales ?? [])
        .filter((sale) =>
          currency === 'all' ? sale.currency === null : sale.currency === currency,
        )
        .slice(0, 10),
    [currency, market?.topItemSales],
  )
  const recentTransactions = useMemo(
    () =>
      (market?.recentTransactions ?? [])
        .filter((entry) => currency === 'all' || entry.currency === currency)
        .slice(0, 10),
    [currency, market?.recentTransactions],
  )
  return (
    <div className="page-scroll">
      <main className="page market-page">
        <PageHeader
          title="Mercado"
          actions={
            <div className="market-reader-status">
              <label>
                Market Reader
                <select
                  value={market?.readerAccountId ?? ''}
                  onChange={(event) => setReader(event.target.value)}
                >
                  <option value="">Automático</option>
                  {eligibleReaderAccounts.map((account) => (
                    <option key={account.id} value={account.id}>
                      {account.nick}
                    </option>
                  ))}
                </select>
              </label>
              <Badge tone={market?.readerAccountId ? 'positive' : 'neutral'}>
                {market?.readerStatus ?? 'Aguardando'}
              </Badge>
              {market?.lastMarketError && (
                <p className="market-reader-error" role="status">
                  {market.lastMarketError}
                </p>
              )}
              {readerError && (
                <p className="market-rule-error" role="alert">
                  {readerError}
                </p>
              )}
              {!eligibleReaderAccounts.length && (
                <p className="market-reader-error">
                  Nenhuma conta conectada e pronta para ler o Mercado.
                </p>
              )}
            </div>
          }
        />

        <section className="market-analysis" aria-label="Análise do Mercado">
          <div className="market-section-heading market-analysis-heading">
            <p className="eyebrow">ANÁLISE DO MERCADO</p>
            <div className="market-analysis-controls">
              <span className="market-period">ÚLTIMAS 24H</span>
              <div
                className="market-filter-buttons"
                role="group"
                aria-label="Filtrar o Mercado por moeda"
              >
                {(
                  [
                    ['all', 'Todas'],
                    ['gold', 'Gold'],
                    ['orb', 'Gemas'],
                  ] as const
                ).map(([value, label]) => (
                  <Button
                    key={value}
                    className={currency === value ? 'selected' : ''}
                    onClick={() => setCurrency(value)}
                  >
                    {label}
                  </Button>
                ))}
              </div>
            </div>
          </div>

          <div className="market-pulse-grid">
            <section
              className="card market-pulse-card"
              aria-label="Itens mais vendidos nas últimas 24 horas"
            >
              <div className="market-pulse-heading">
                <h3>Mais vendidos</h3>
                <small>{market?.historyStatus ?? 'Aguardando'}</small>
              </div>
              <div className="market-top-sales-head" aria-hidden="true">
                <span className="market-top-sales-item-heading">ITEM</span>
                <span>VENDAS</span>
                <span>MÉDIA GOLD</span>
                <span>MÉDIA GEMAS</span>
                <span>UNIDADES</span>
              </div>
              <ol className="market-pulse-list">
                {topItemSales.map((sale, index) => {
                  const inventoryItem = marketItemForName(sale.itemName, catalog)
                  const averageGold =
                    sale.currency === 'gold' ? sale.averageUnitPrice : sale.averageGoldUnitPrice
                  const averageGems =
                    sale.currency === 'orb' ? sale.averageUnitPrice : sale.averageOrbUnitPrice
                  return (
                    <li
                      key={`${sale.currency ?? 'all'}-${sale.itemName}`}
                      className="market-pulse-row"
                    >
                      <span className="market-pulse-rank">{index + 1}</span>
                      <span className="market-pulse-asset">
                        <ItemAsset item={inventoryItem} catalog={catalog} compact />
                      </span>
                      <span className="market-pulse-copy">
                        <b>{sale.itemName}</b>
                      </span>
                      <strong className="market-top-sales-value">
                        {format.format(sale.transactions)}
                      </strong>
                      <strong className="market-top-sales-value market-gold-value">
                        {averageGold === null ? (
                          '—'
                        ) : (
                          <>
                            {format.format(averageGold)}
                            <span className="market-sale-currency-icon">
                              <MarketCurrencyIcon currency="gold" />
                            </span>
                          </>
                        )}
                      </strong>
                      <strong className="market-top-sales-value market-gem-value">
                        {averageGems === null ? (
                          '—'
                        ) : (
                          <>
                            {format.format(averageGems)}
                            <span className="market-sale-currency-icon">
                              <MarketCurrencyIcon currency="orb" />
                            </span>
                          </>
                        )}
                      </strong>
                      <strong className="market-top-sales-value">
                        {format.format(sale.quantity)}
                      </strong>
                    </li>
                  )
                })}
                {!topItemSales.length && (
                  <li className="market-pulse-empty">
                    {market
                      ? 'Coletando o histórico do Mercado...'
                      : 'Aguardando a primeira leitura.'}
                  </li>
                )}
              </ol>
            </section>

            <section
              className="card market-pulse-card market-latest-sales-card"
              aria-label="Últimas vendas do Mercado"
            >
              <div className="market-pulse-heading">
                <h3>Últimas vendas</h3>
                <small>
                  {market?.historyLastUpdatedAt
                    ? `Atualizado ${new Date(market.historyLastUpdatedAt).toLocaleTimeString('pt-BR')}`
                    : 'Aguardando'}
                </small>
              </div>
              <div className="market-sales-head" aria-hidden="true">
                <span />
                <span>PREÇO UNITÁRIO</span>
                <span>PREÇO FINAL</span>
              </div>
              <ol className="market-pulse-list">
                {recentTransactions.map((entry) => {
                  const unitPrice = marketUnitPrice(entry)
                  const priceColor =
                    entry.currency === 'gold' ? 'market-gold-value' : 'market-gem-value'
                  return (
                    <li key={entry.id} className="market-pulse-row market-recent-sale-row">
                      <span className="market-sale-product">
                        <span className="market-pulse-asset">
                          <MarketHistoryAsset entry={entry} catalog={catalog} />
                        </span>
                        <span className="market-pulse-copy">
                          <b>{marketHistoryLabel(entry)}</b>
                          <small>
                            {entry.buyer || 'Comprador não identificado'} ·{' '}
                            {new Date(entry.occurredAt).toLocaleTimeString('pt-BR', {
                              hour: '2-digit',
                              minute: '2-digit',
                            })}
                          </small>
                        </span>
                      </span>
                      <strong className={`market-sale-price market-sale-unit-price ${priceColor}`}>
                        {unitPrice === null ? (
                          '—'
                        ) : (
                          <>
                            {format.format(unitPrice)}
                            <span className="market-sale-currency-icon">
                              <MarketCurrencyIcon currency={entry.currency} />
                            </span>
                          </>
                        )}
                      </strong>
                      <strong className={`market-sale-price market-sale-total-price ${priceColor}`}>
                        {format.format(entry.total)}
                        <span className="market-sale-currency-icon">
                          <MarketCurrencyIcon currency={entry.currency} />
                        </span>
                      </strong>
                    </li>
                  )
                })}
                {!recentTransactions.length && (
                  <li className="market-pulse-empty">
                    {market ? 'Coletando as últimas vendas...' : 'Aguardando a primeira leitura.'}
                  </li>
                )}
              </ol>
            </section>
          </div>

          <div className="market-offers-heading">
            <h2>Ofertas ativas</h2>
            <span>Coleta única · intervalo de 30 s</span>
          </div>
          <div className="market-filters">
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Buscar item"
              aria-label="Buscar item no Mercado"
            />
            <small>
              {market?.lastUpdatedAt
                ? `Atualizado ${new Date(market.lastUpdatedAt).toLocaleTimeString('pt-BR')}`
                : 'Sem leitura ainda'}
            </small>
          </div>
          <div className="market-table" role="table" aria-label="Ofertas atuais do Mercado">
            <div className="market-table-head" role="row">
              <span>ITEM</span>
              <span>OFERTAS</span>
              <span>UNIDADES</span>
              <span>MENOR GOLD</span>
              <span>MENOR GEMA</span>
            </div>
            <div
              className="market-table-scroll"
              role="rowgroup"
              tabIndex={0}
              aria-label="Lista rolável de ofertas ativas"
              onScroll={(event) => setMarketRowsScrollTop(event.currentTarget.scrollTop)}
            >
              <div
                className="market-table-virtualizer"
                style={{ height: rows.length * marketRowHeight }}
              >
                <div style={{ transform: `translateY(${marketRowStart * marketRowHeight}px)` }}>
                  {visibleMarketRows.map(({ summary, item }) => {
                    const inventoryItem = item
                    return (
                      <div key={summary.itemId} className="market-table-row" role="row">
                        <span className="market-item">
                          <ItemAsset item={inventoryItem} catalog={catalog} compact />
                          <b>{inventoryItem.name}</b>
                        </span>
                        <span>{format.format(summary.listings)}</span>
                        <span>{format.format(summary.units)}</span>
                        <span className="market-gold-value">
                          {summary.minGold === null ? '—' : format.format(summary.minGold)}
                        </span>
                        <span className="market-gem-value">
                          {summary.minOrb === null ? '—' : format.format(summary.minOrb)}
                        </span>
                      </div>
                    )
                  })}
                </div>
              </div>
              {!rows.length && (
                <p className="market-empty">
                  {market
                    ? 'Nenhuma oferta corresponde ao filtro.'
                    : 'Aguardando a primeira coleta do leitor.'}
                </p>
              )}
            </div>
          </div>
        </section>

        <section className="market-sniper" aria-label="Compra automática no Mercado">
          <div className="market-section-heading">
            <div>
              <p className="eyebrow">COMPRA AUTOMÁTICA</p>
              <h2>O que cada conta deve procurar?</h2>
            </div>
          </div>
          <section ref={ruleEditorRef} className="card market-rule-editor">
            <div className="market-rule-editor-copy">
              <p className="eyebrow">{editingRuleId ? 'EDITAR COMPRA' : 'NOVA COMPRA'}</p>
            </div>
            <div className="market-rule-main-fields">
              <label>
                Conta
                <select
                  value={draft.accountId}
                  onChange={(event) => setDraft({ ...draft, accountId: event.target.value })}
                >
                  {accounts.map((account) => (
                    <option key={account.id} value={account.id}>
                      {account.nick}
                    </option>
                  ))}
                </select>
              </label>
              <div
                className="market-item-picker"
                onBlur={(event) => {
                  const nextFocus = event.relatedTarget as Node | null
                  if (!nextFocus || !event.currentTarget.contains(nextFocus))
                    setItemPickerOpen(false)
                }}
              >
                <label>
                  Item
                  <input
                    role="combobox"
                    aria-expanded={itemPickerOpen}
                    aria-autocomplete="list"
                    aria-haspopup="listbox"
                    aria-controls="market-item-options"
                    value={itemSearch}
                    placeholder={selectedDraftItem?.name ?? 'Buscar item'}
                    onFocus={() => setItemPickerOpen(true)}
                    onChange={(event) => {
                      setItemSearch(event.target.value)
                      setItemPickerScrollTop(0)
                      setItemPickerOpen(true)
                      if (draft.itemId) setDraft({ ...draft, itemId: '' })
                    }}
                  />
                </label>
                {selectedDraftItem && !itemSearch && (
                  <span className="market-selected-item">
                    <ItemAsset item={selectedDraftItem} catalog={catalog} compact />
                    {selectedDraftItem.name}
                  </span>
                )}
                {itemPickerOpen && (
                  <div
                    id="market-item-options"
                    className="market-item-options"
                    role="listbox"
                    aria-label="Itens encontrados"
                    onScroll={(event) => setItemPickerScrollTop(event.currentTarget.scrollTop)}
                  >
                    <div
                      className="market-picker-virtualizer"
                      style={{ height: matchingItems.length * pickerRowHeight }}
                    >
                      <div
                        style={{ transform: `translateY(${pickerRowStart * pickerRowHeight}px)` }}
                      >
                        {visiblePickerItems.map((item) => (
                          <button
                            key={item.id}
                            type="button"
                            role="option"
                            aria-selected={String(item.id) === draft.itemId}
                            onClick={() => {
                              setDraft({ ...draft, itemId: String(item.id) })
                              setItemSearch('')
                              setItemPickerScrollTop(0)
                              setItemPickerOpen(false)
                            }}
                          >
                            <ItemAsset item={item} catalog={catalog} compact />
                            <span>{item.name}</span>
                          </button>
                        ))}
                      </div>
                    </div>
                    {!matchingItems.length && (
                      <p>
                        {marketItemIds.size
                          ? 'Nenhum item encontrado.'
                          : 'Aguardando a primeira coleta do Mercado.'}
                      </p>
                    )}
                  </div>
                )}
              </div>
              <label>
                Pagar com
                <select
                  value={draft.currency}
                  onChange={(event) =>
                    setDraft({ ...draft, currency: event.target.value as MarketCurrency })
                  }
                >
                  <option value="gold">Gold</option>
                  <option value="orb">Gemas</option>
                </select>
              </label>
              <label>
                Preço máximo por unidade
                <input
                  inputMode="numeric"
                  value={draft.maxPrice}
                  placeholder="Ex.: 62.000"
                  onChange={(event) => setDraft({ ...draft, maxPrice: event.target.value })}
                />
              </label>
            </div>
            <details className="market-rule-advanced">
              <summary>
                Proteções avançadas <span>opcional</span>
              </summary>
              <div>
                <label>
                  Limite total de gastos
                  <input
                    inputMode="numeric"
                    value={draft.budget}
                    placeholder="Sem limite"
                    onChange={(event) => setDraft({ ...draft, budget: event.target.value })}
                  />
                  <small>Compras confirmadas continuam contando ao editar este limite.</small>
                </label>
                <label>
                  Saldo mínimo a manter
                  <input
                    inputMode="numeric"
                    value={draft.minimumBalance}
                    placeholder="Sem saldo mínimo"
                    onChange={(event) => setDraft({ ...draft, minimumBalance: event.target.value })}
                  />
                </label>
              </div>
            </details>
            <div className="market-rule-create">
              <p>
                {selectedDraftItem ? (
                  <>
                    <b>
                      {accounts.find((account) => account.id === draft.accountId)?.nick ??
                        'A conta'}
                    </b>{' '}
                    comprará {selectedDraftItem.name} disponível por até{' '}
                    <b>
                      {draft.maxPrice || '—'} {draft.currency === 'gold' ? 'Gold' : 'Gemas'}
                    </b>{' '}
                    a unidade.
                  </>
                ) : (
                  'Escolha um item para criar sua compra automática.'
                )}
              </p>
              <div className="market-rule-create-actions">
                <Button onClick={createRule}>
                  {editingRuleId ? 'Salvar alterações' : 'Criar compra automática'}
                </Button>
                {editingRuleId && <Button onClick={resetRuleEditor}>Cancelar</Button>}
              </div>
            </div>
            {ruleError && (
              <p className="market-rule-error" role="alert">
                {ruleError}
              </p>
            )}
          </section>
          <div className="market-rules">
            {(market?.rules ?? []).map((rule) => {
              const item = marketItemForId(rule.itemId, catalog)
              const account = accounts.find((candidate) => candidate.id === rule.accountId)
              const remainingBudget = Math.max(0, rule.budget - rule.spent)
              const budgetProgress =
                rule.budget > 0 ? Math.min(100, (rule.spent / rule.budget) * 100) : 0
              return (
                <Card key={rule.id} className="market-rule">
                  <div className="market-rule-identity">
                    <p className="eyebrow">
                      {account?.nick ?? 'Conta removida'} ·{' '}
                      {rule.currency === 'gold' ? 'GOLD' : 'GEMAS'}
                    </p>
                    <h3 className="market-rule-title">
                      <ItemAsset item={item} catalog={catalog} compact />
                      <span>{item.name}</span>
                    </h3>
                  </div>
                  <dl
                    className="market-rule-metrics"
                    aria-label={`Detalhes da compra de ${item.name}`}
                  >
                    <div>
                      <dt>MÁX. / UNIDADE</dt>
                      <dd>
                        {format.format(rule.maxPrice)}{' '}
                        <small>{rule.currency === 'gold' ? 'Gold' : 'Gemas'}</small>
                      </dd>
                    </div>
                    <div>
                      <dt>COMPRADAS</dt>
                      <dd>{format.format(rule.purchasedQuantity)}</dd>
                    </div>
                    {rule.budget > 0 ? (
                      <div className="market-rule-budget">
                        <dt>ORÇAMENTO RESTANTE</dt>
                        <dd>
                          {format.format(remainingBudget)}{' '}
                          <small>de {format.format(rule.budget)}</small>
                        </dd>
                        <span
                          className="market-rule-budget-track"
                          role="progressbar"
                          aria-label="Orçamento usado"
                          aria-valuemin={0}
                          aria-valuemax={rule.budget}
                          aria-valuenow={Math.min(rule.spent, rule.budget)}
                        >
                          <i style={{ width: `${budgetProgress}%` }} />
                        </span>
                      </div>
                    ) : (
                      <div className="market-rule-protection">
                        <dt>LIMITE TOTAL</dt>
                        <dd>Sem limite</dd>
                      </div>
                    )}
                    {rule.minimumBalance > 0 && (
                      <div className="market-rule-protection">
                        <dt>RESERVA</dt>
                        <dd>{format.format(rule.minimumBalance)}</dd>
                      </div>
                    )}
                  </dl>
                  <div className="market-rule-actions">
                    <Switch
                      checked={rule.enabled}
                      label={`Ativar compra`}
                      onCheckedChange={(enabled) => saveRule({ ...rule, enabled })}
                    />
                    <Button onClick={() => editRule(rule)}>Editar</Button>
                    <Button
                      onClick={() =>
                        void invoke('delete_market_sniper_rule', { ruleId: rule.id })
                          .then(refresh)
                          .catch(() => undefined)
                      }
                    >
                      Remover
                    </Button>
                  </div>
                </Card>
              )
            })}
            {!market?.rules.length && (
              <p className="market-empty">
                Nenhuma compra automática configurada. Comece escolhendo o primeiro item acima.
              </p>
            )}
          </div>
          <section
            className="market-purchase-history"
            aria-label="Histórico de compras e sorteios do Sniper"
          >
            <div className="market-history-heading">
              <div>
                <p className="eyebrow">HISTÓRICO LOCAL</p>
                <h3>Compras e sorteios</h3>
              </div>
              <label>
                Conta
                <select
                  value={purchaseAccountFilter}
                  onChange={(event) => setPurchaseAccountFilter(event.target.value)}
                >
                  <option value="all">Todas as contas</option>
                  {accounts.map((account) => (
                    <option key={account.id} value={account.id}>
                      {account.nick}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <p>
              Mostra compras concluídas e ofertas que perderam o sorteio. Sorteios perdidos não
              descontam saldo.
            </p>
            <div
              className="market-history-list"
              onScroll={(event) => setPurchaseHistoryScrollTop(event.currentTarget.scrollTop)}
            >
              <div
                className="market-history-virtualizer"
                style={{ height: visiblePurchases.length * MARKET_HISTORY_ROW_HEIGHT }}
              >
                <div
                  style={{
                    transform: `translateY(${historyRowStart * MARKET_HISTORY_ROW_HEIGHT}px)`,
                  }}
                >
                  {renderedPurchases.map((purchase) => {
                    const item = catalog.items[String(purchase.itemId)]
                    const account = accounts.find(
                      (candidate) => candidate.id === purchase.accountId,
                    )
                    const inventoryItem = {
                      id: String(purchase.itemId),
                      assetKey: String(purchase.itemId),
                      name: item?.name ?? purchase.description,
                      quantity: 0,
                      category: 'other' as const,
                    }
                    const lostLottery = purchase.outcome === 'lostLottery'
                    const uncertain = purchase.outcome === 'uncertain'
                    return (
                      <div
                        key={purchase.id}
                        className={`market-history-row${lostLottery ? ' lost-lottery' : ''}${uncertain ? ' uncertain-result' : ''}`}
                      >
                        <span className="market-history-item">
                          <ItemAsset item={inventoryItem} catalog={catalog} compact />
                          <b>{purchase.description}</b>
                        </span>
                        <span>{account?.nick ?? 'Conta removida'}</span>
                        <span>{new Date(purchase.purchasedAt).toLocaleString('pt-BR')}</span>
                        <span className="market-history-outcome">
                          <span
                            className={`market-history-status ${lostLottery ? 'lost' : uncertain ? 'uncertain' : 'purchased'}`}
                          >
                            {lostLottery
                              ? 'Perdeu sorteio'
                              : uncertain
                                ? 'Resultado incerto'
                                : 'Comprada'}
                          </span>
                          <strong
                            className={
                              purchase.currency === 'gold'
                                ? 'market-gold-value'
                                : 'market-gem-value'
                            }
                          >
                            {(lostLottery || uncertain) && (
                              <small>{uncertain ? 'Não confirmado · ' : 'Oferta '}</small>
                            )}
                            {format.format(purchase.total)}
                            <span className="market-sale-currency-icon">
                              <MarketCurrencyIcon currency={purchase.currency} />
                            </span>
                          </strong>
                        </span>
                      </div>
                    )
                  })}
                </div>
              </div>
              {!visiblePurchases.length && (
                <p className="market-empty">Ainda não há resultados para este filtro.</p>
              )}
            </div>
            {market?.purchaseHistoryTruncated && (
              <p className="market-history-truncated">Exibindo os 200 resultados mais recentes.</p>
            )}
          </section>
        </section>
      </main>
    </div>
  )
}
function BootScreen({ status = 'Iniciando…' }: { status?: string }) {
  return (
    <main className="boot-screen" role="status" aria-live="polite">
      <span className="boot-brand-mark" aria-hidden="true">
        ◉
      </span>
      <span className="boot-brand-name">POKEIDLE MANAGER</span>
      <span className="boot-spinner" aria-hidden="true" />
      <span className="boot-status">{status}</span>
    </main>
  )
}

function PreparingShell() {
  return (
    <>
      <CustomTitleBar />
      <div className="app-shell startup-shell">
        <aside>
          <div className="brand">
            <img className="sidebar-brand-art" src={sidebarLogo} alt="Pokeidle Manager" />
          </div>
          <nav aria-label="Navegação">
            {pages.map((item) => (
              <button key={item} type="button" disabled>
                <span
                  className={`sidebar-nav-icon sidebar-nav-icon-${pages.indexOf(item)}`}
                  aria-hidden="true"
                >
                  <SidebarNavIcon page={item} />
                </span>
                <span className="sidebar-nav-label">{item}</span>
              </button>
            ))}
          </nav>
          <div className="sidebar-wallpaper" aria-hidden="true"><img src={sidebarWallpaper} alt="" /></div>
          <footer>
            <span className="pulse" /> Núcleo local
          </footer>
        </aside>
        <div className="content">
          <main className="page startup-page" role="status" aria-live="polite">
            <p className="eyebrow">INICIALIZAÇÃO</p>
            <h1>Preparando dados…</h1>
            <span className="boot-spinner" aria-hidden="true" />
          </main>
        </div>
      </div>
    </>
  )
}

function Settings() {
  const { settings, setSetting } = useAppStore()
  const concurrencyProfiles = [
    [1, 'Menor uso de memória'],
    [2, 'Equilibrado'],
    [3, 'Inicialização mais rápida'],
    [4, 'Mais rápido · maior pico'],
  ] as const
  return (
    <main className="page settings">
      <PageHeader title="Configurações" />
      <Card className="startup-concurrency-card">
        <div className="startup-concurrency-heading">
          <div>
            <p className="eyebrow">INICIALIZAÇÃO</p>
            <h2>Contas simultâneas</h2>
          </div>
          <p>Define quantas contas persistidas podem iniciar em Background ao mesmo tempo.</p>
        </div>
        <div className="startup-concurrency-options" role="group" aria-label="Contas simultâneas">
          {concurrencyProfiles.map(([value, description]) => (
            <button
              key={value}
              type="button"
              className={settings.startupBrowserConcurrency === value ? 'selected' : ''}
              aria-pressed={settings.startupBrowserConcurrency === value}
              onClick={() => setSetting('startupBrowserConcurrency', value)}
            >
              <strong>{value}</strong>
              <span>{description}</span>
            </button>
          ))}
        </div>
        <p className="startup-concurrency-note">Aplicado na próxima inicialização.</p>
      </Card>
      <Card className="community-project-card">
        <div className="community-project-heading">
          <div>
            <p className="eyebrow">PROJETO OPEN SOURCE</p>
            <h2>Pokeidle Manager</h2>
            <p className="muted">Versão {communityInfo.version}</p>
          </div>
          <Badge tone="positive">Community Build</Badge>
        </div>
        <div className="community-project-status" aria-label="Sobre o projeto">
          <span>✓ Gratuito</span>
          <span>✓ Código aberto · GPL-3.0-only</span>
          <span>✓ Sem ativação comercial</span>
        </div>
        <p className="muted">
          O repositório e os canais de suporte serão adicionados quando estiverem confirmados.
        </p>
        <div className="community-project-actions">
          <Button disabled={!communityInfo.githubUrl} title="Endereço ainda não configurado">
            Ver no GitHub
          </Button>
          <Button disabled={!communityInfo.discordUrl} title="Endereço ainda não configurado">
            Discord — configurar URL
          </Button>
          <Button disabled={!communityInfo.pixEnabled} title="Apoio via PIX ainda não configurado">
            Apoie o projeto · PIX em breve
          </Button>
        </div>
      </Card>
      <Card>
        <Switch
          label="Minimizar para bandeja"
          checked={settings.minimizeToTray}
          onCheckedChange={(value) => setSetting('minimizeToTray', value)}
        />
        <Switch
          label="Modo Demonstração"
          checked={settings.mockMode}
          onCheckedChange={(value) => setSetting('mockMode', value)}
        />
        <Switch
          label="Modo Desenvolvedor"
          checked={settings.developerMode}
          onCheckedChange={(value) => setSetting('developerMode', value)}
        />
      </Card>
      {settings.developerMode && <ProtocolInspector />}
      <Card>
        <label className="field">
          GAME_URL
          <input
            value={settings.gameUrl}
            onChange={(event) => setSetting('gameUrl', event.target.value)}
          />
        </label>
        <Button onClick={() => alert('A pasta de logs será aberta pela integração desktop.')}>
          Abrir pasta de logs
        </Button>
        <p className="muted">
          Pokeidle Manager {communityInfo.version} · Iniciar com Windows permanece preparado para uma próxima versão.
        </p>
      </Card>
    </main>
  )
}

type InspectorDirection = 'clientToServer' | 'serverToClient'
type InspectorFrame = { timestampMs: number; direction: InspectorDirection; payload: string }
function ProtocolInspector() {
  const accounts = useAppStore(selectVisibleAccounts)
  const [accountId, setAccountId] = useState('')
  const [direction, setDirection] = useState<'all' | InspectorDirection>('all')
  const [query, setQuery] = useState('')
  const [frames, setFrames] = useState<InspectorFrame[]>([])
  const [paused, setPaused] = useState(false)
  useEffect(() => {
    if (!accountId && accounts[0]) setAccountId(accounts[0].id)
    if (accountId && !accounts.some((account) => account.id === accountId))
      setAccountId(accounts[0]?.id ?? '')
  }, [accountId, accounts])
  useEffect(() => {
    if (!accountId || paused || !('__TAURI_INTERNALS__' in window)) return
    const refresh = () =>
      void invoke<InspectorFrame[]>('protocol_inspector_frames', { accountId }).then(setFrames)
    refresh()
    const timer = window.setInterval(refresh, 750)
    return () => window.clearInterval(timer)
  }, [accountId, paused])
  const visible = useMemo(
    () =>
      frames.filter(
        (frame) =>
          (direction === 'all' || frame.direction === direction) &&
          frame.payload.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
      ),
    [direction, frames, query],
  )
  const clear = () => {
    if (!accountId) return
    void invoke('clear_protocol_inspector_frames', { accountId }).then(() => setFrames([]))
  }
  const copy = (payload: string) => void navigator.clipboard?.writeText(payload)
  return (
    <Card className="protocol-inspector">
      <div className="inspector-heading">
        <div>
          <p className="eyebrow">DESENVOLVIMENTO</p>
          <h2>Protocol Inspector</h2>
        </div>
        <small>até 500 frames em memória</small>
      </div>
      <label className="field">
        Conta
        <select value={accountId} onChange={(event) => setAccountId(event.target.value)}>
          {accounts.map((account) => (
            <option key={account.id} value={account.id}>
              {account.nick}
            </option>
          ))}
        </select>
      </label>
      <div className="inspector-filters">
        {(
          [
            ['all', 'Todos'],
            ['clientToServer', 'C→S'],
            ['serverToClient', 'S→C'],
          ] as const
        ).map(([value, label]) => (
          <Button
            key={value}
            className={direction === value ? 'selected' : ''}
            onClick={() => setDirection(value)}
          >
            {label}
          </Button>
        ))}
        <input
          aria-label="Filtrar frames"
          placeholder="Filtro"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      </div>
      <div className="inspector-actions">
        <Button onClick={clear}>Limpar</Button>
        <Button onClick={() => setPaused((value) => !value)}>
          {paused ? 'Retomar visualização' : 'Pausar visualização'}
        </Button>
      </div>
      <div className="frame-list" aria-label="Frames do protocolo">
        {visible.map((frame, index) => (
          <button
            key={`${frame.timestampMs}-${index}`}
            className="protocol-frame"
            onClick={() => copy(frame.payload)}
            title="Copiar frame sanitizado"
          >
            <time>{new Date(frame.timestampMs).toLocaleTimeString('pt-BR')}</time>
            <b>{frame.direction === 'clientToServer' ? 'C→S' : 'S→C'}</b>
            <code>{frame.payload}</code>
          </button>
        ))}
        {!visible.length && <p className="muted">Nenhum frame capturado para esta conta.</p>}
      </div>
    </Card>
  )
}

export default function App() {
  const isDesktop = '__TAURI_INTERNALS__' in window
  const { page, detailAccountId } = useAppStore((state) => state.ui),
    { setPage, settings, advanceMock, hydratePersistedSettings, setRealAccounts } = useAppStore(),
    accounts = useAppStore(selectVisibleAccounts)
  const [diagnostic, setDiagnostic] = useState<IntegrationDiagnostic | null>(null)
  const [browserDialog, setBrowserDialog] = useState<{ intent: BrowserIntent; issue: BrowserIssue; busy: boolean; notice: string | null } | null>(null)
  const [browserActionBusy, setBrowserActionBusy] = useState(false)
  const [addAccountError, setAddAccountError] = useState<string | null>(null)
  const browserActionLock = useRef(false)
  const pendingAddAccountId = useRef<string | null>(null)
  const integrationSnapshotSequence = useRef(0)
  const pendingAddSnapshotBaseline = useRef(0)
  const [canRetryFailedAdd, setCanRetryFailedAdd] = useState(false)
  const [coreStatus, setCoreStatus] = useState<{ ready: boolean; error: string | null }>(() => ({
    ready: !isDesktop,
    error: null,
  }))
  const [hydrationComplete, setHydrationComplete] = useState(false)
  const [persistedSettingsLoaded, setPersistedSettingsLoaded] = useState(false)
  const firstRender = useRef(true)
  if (firstRender.current) {
    firstRender.current = false
    if (startupDebug) console.info('[startup] App first render', performance.now().toFixed(1))
  }
  const appIsReady = !isDesktop || coreStatus.ready
  const settlePendingAdd = useCallback((snapshot: IntegrationLiveSnapshot, requestSequence: number) => {
    const pendingId = pendingAddAccountId.current
    const diagnostic = snapshot.diagnostic
    if (!pendingId || requestSequence <= pendingAddSnapshotBaseline.current) return
    const diagnosticMatches = diagnostic.accountId === pendingId
    const account = snapshot.accounts.find((entry) => entry.account.id === pendingId)?.account
    const fail = (message: string) => {
      pendingAddAccountId.current = null
      browserActionLock.current = false
      setBrowserActionBusy(false)
      setAddAccountError(message)
      setCanRetryFailedAdd(true)
    }
    const succeed = () => {
      pendingAddAccountId.current = null
      browserActionLock.current = false
      setBrowserActionBusy(false)
      setAddAccountError(null)
      setCanRetryFailedAdd(false)
    }

    if (diagnosticMatches && (
      diagnostic.welcomeReceived
      || diagnostic.lifecycle === 'authenticated'
      || diagnostic.lifecycle === 'readyForHandoff'
      || (diagnostic.lifecycle === 'waitingForLogin' && diagnostic.braveStarted && diagnostic.gameUrlNavigated)
    )) {
      succeed()
      return
    }

    if (diagnosticMatches && (
      diagnostic.lifecycle === 'error'
      || diagnostic.lifecycle === 'closed'
      || diagnostic.state === 'Controle do navegador indisponível'
    )) {
      fail(diagnostic.message.trim()
        ? `Não foi possível concluir a conexão: ${diagnostic.message}`
        : 'Não foi possível concluir a conexão da conta. Tente novamente.')
      return
    }

    if (account) {
      if (account.runtime === 'browser_connected' || account.status === 'online') {
        succeed()
        return
      }
      if (account.runtime === 'error' || account.runtime === 'offline' || account.runtime === 'stopping' || account.status === 'error' || account.status === 'offline') {
        fail('A inicialização do navegador terminou antes de conectar. Tente novamente.')
      }
      return
    }

    if (!diagnosticMatches) {
      fail('A conta não apareceu na atualização do navegador. Tente novamente.')
    }
  }, [])
  useEffect(() => {
    if (!isDesktop) return
    let disposed = false
    let unlisten: (() => void) | undefined
    const applyStatus = (status: { ready: boolean; error: string | null }) => {
      if (!disposed) setCoreStatus(status)
    }
    void listen<{ ready: boolean; error: string | null }>('app-core-state-changed', (event) => {
      applyStatus(event.payload)
    }).then((stop) => {
      if (disposed) stop()
      else unlisten = stop
    })
    void invoke<{ ready: boolean; error: string | null }>('app_core_status')
      .then(applyStatus)
      .catch(() =>
        applyStatus({ ready: false, error: 'Não foi possível preparar os dados do aplicativo.' }),
      )
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [isDesktop])
  useEffect(() => {
    if (!startupDebug) return
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        console.info('[startup] first React paint', performance.now().toFixed(1))
      }),
    )
  }, [])
  useEffect(() => {
    if (!settings.mockMode || !appIsReady) return
    const timer = window.setInterval(advanceMock, 1500)
    return () => window.clearInterval(timer)
  }, [advanceMock, appIsReady, settings.mockMode])
  useEffect(() => {
    if (!isDesktop || !appIsReady) return
    if (startupDebug)
      console.info('[startup] persisted accounts hydration start', performance.now().toFixed(1))
    void invoke<LiveRuntimeSnapshot[]>('dashboard_live')
      .then((accounts) => {
        setRealAccounts(accounts.map(accountFromLive))
        setHydrationComplete(true)
        if (startupDebug)
          console.info('[startup] persisted accounts hydration end', performance.now().toFixed(1))
      })
      .catch(() => setHydrationComplete(true))
  }, [appIsReady, isDesktop, setRealAccounts])
  useEffect(() => {
    if (!isDesktop || !appIsReady) return
    let disposed = false
    let timer: number | undefined
    const refresh = async () => {
      try {
        const snapshot = await invoke<MarketSnapshot>('market_snapshot')
        if (!disposed) useMarketStore.getState().setSnapshot(snapshot)
      } catch {
        // O snapshot anterior continua visível enquanto a ponte local se recupera.
      } finally {
        if (!disposed) timer = window.setTimeout(refresh, MARKET_SNAPSHOT_REFRESH_MS)
      }
    }
    void refresh()
    return () => {
      disposed = true
      if (timer !== undefined) window.clearTimeout(timer)
    }
  }, [appIsReady, isDesktop])
  useEffect(() => {
    if (!isDesktop || !appIsReady || settings.mockMode) return
    // This runs after React has committed a visible shell. Browser/CDP startup
    // is intentionally downstream of the first paint, never a prerequisite.
    let disposed = false
    let timer: number | undefined
    const frame = requestAnimationFrame(() => {
      timer = window.setTimeout(() => {
        if (disposed || useAppStore.getState().settings.mockMode) return
        if (startupDebug)
          console.info(
            '[startup] first paint complete; scheduling account bootstrap',
            performance.now().toFixed(1),
          )
        void invoke<LiveRuntimeSnapshot[]>('schedule_restored_account_live_bootstraps')
          .then((accounts) => {
            if (!disposed) setRealAccounts(accounts.map(accountFromLive))
          })
          .catch((error: unknown) => {
            if (!disposed && startupDebug)
              console.warn('[startup] account bootstrap scheduling failed', error)
          })
      }, 0)
    })
    return () => {
      disposed = true
      cancelAnimationFrame(frame)
      if (timer !== undefined) window.clearTimeout(timer)
    }
  }, [appIsReady, isDesktop, setRealAccounts, settings.mockMode])
  useEffect(() => {
    if (!isDesktop || !appIsReady) return
    if (startupDebug)
      console.info('[startup] app settings load start', performance.now().toFixed(1))
    void invoke<{
      minimizeToTray: boolean
      gameUrl: string
      startupBrowserConcurrency: number
    }>('settings_snapshot')
      .then((persisted) => {
        hydratePersistedSettings(persisted)
        setPersistedSettingsLoaded(true)
        if (startupDebug)
          console.info('[startup] app settings load end', performance.now().toFixed(1))
      })
      .catch(() => setPersistedSettingsLoaded(true))
  }, [appIsReady, hydratePersistedSettings, isDesktop])
  useEffect(() => {
    if (!isDesktop || !appIsReady || !persistedSettingsLoaded) return
    void invoke('save_app_settings', {
      settings: {
        minimizeToTray: settings.minimizeToTray,
        gameUrl: settings.gameUrl,
        startupBrowserConcurrency: settings.startupBrowserConcurrency,
      },
    })
  }, [
    persistedSettingsLoaded,
    isDesktop,
    appIsReady,
    settings.gameUrl,
    settings.minimizeToTray,
    settings.startupBrowserConcurrency,
  ])
  useEffect(() => {
    if (!isDesktop || !appIsReady) return
    void invoke('set_protocol_inspector_enabled', { enabled: settings.developerMode })
  }, [appIsReady, isDesktop, settings.developerMode])
  useEffect(() => {
    if (!isDesktop || !appIsReady) return
    let disposed = false
    let timer: number | undefined
    const refresh = async () => {
      const requestSequence = ++integrationSnapshotSequence.current
      try {
        const snapshot = await invoke<IntegrationLiveSnapshot>('integration_live_snapshot')
        if (disposed) return
        setDiagnostic(snapshot.diagnostic)
        setRealAccounts(snapshot.accounts.map(accountFromLive))
        settlePendingAdd(snapshot, requestSequence)
        if (startupDebug && snapshot.diagnostic.welcomeReceived)
          console.info('[startup] first welcome observed', performance.now().toFixed(1))
      } catch {
        // A transient backend delay must not create a queue of overlapping IPC calls.
      } finally {
        if (!disposed) timer = window.setTimeout(refresh, 1_200)
      }
    }
    void refresh()
    return () => {
      disposed = true
      if (timer !== undefined) window.clearTimeout(timer)
    }
  }, [appIsReady, isDesktop, setRealAccounts, settlePendingAdd])
  const runBrowserIntent = async (intent: BrowserIntent, isRetry = false) => {
    if (browserActionLock.current) return
    let keepAddLockUntilSnapshot = false
    browserActionLock.current = true
    setBrowserActionBusy(true)
    setAddAccountError(null)
    if (intent.kind === 'add') setCanRetryFailedAdd(false)
    setBrowserDialog((current) => current ? { ...current, busy: true, notice: null } : current)
    try {
      if (intent.kind === 'add') {
        // The backend resolves Brave before creating any account/profile and guards this boundary against races.
        const result = await invoke<StartAccountResult>('start_real_account')
        if (result.status !== 'started') {
          if (result.status === 'limitReached') {
            setAddAccountError('O limite de 4 contas foi atingido.')
            setBrowserDialog(null)
            return
          }
          setBrowserDialog({
            intent,
            issue: result.status === 'browserNotFound' ? 'notFound' : 'invalidInstallation',
            busy: false,
            notice: isRetry && result.status === 'browserNotFound' ? 'Brave ainda não foi encontrado.' : null,
          })
          return
        }
        pendingAddAccountId.current = result.accountId
        pendingAddSnapshotBaseline.current = integrationSnapshotSequence.current
        keepAddLockUntilSnapshot = true
        setBrowserDialog(null)
        return
      }

      await invoke(intent.command, { accountId: intent.accountId })
      setBrowserDialog(null)
    } catch (error) {
      const issue = browserIssueFromError(error)
      if (issue) {
        setBrowserDialog({
          intent,
          issue,
          busy: false,
          notice: isRetry && issue === 'notFound' ? 'Brave ainda não foi encontrado.' : null,
        })
        return
      }
      console.warn('[browser] Ação de navegador falhou.', error)
      if (intent.kind === 'add') {
        setAddAccountError('Não foi possível iniciar a conexão. Tente novamente.')
        setCanRetryFailedAdd(true)
        setBrowserDialog(null)
      } else {
        throw error
      }
    } finally {
      if (!keepAddLockUntilSnapshot) {
        browserActionLock.current = false
        setBrowserActionBusy(false)
      }
      setBrowserDialog((current) => current ? { ...current, busy: false } : current)
    }
  }
  const startRealAccount = () => void runBrowserIntent({ kind: 'add' })
  const retryFailedAdd = () => {
    if (browserActionLock.current || !canRetryFailedAdd) return
    setCanRetryFailedAdd(false)
    startRealAccount()
  }
  const runAccountBrowserAction = (accountId: string, command: BrowserAccountCommand) =>
    runBrowserIntent({ kind: 'account', accountId, command })
  const retryBrowserIntent = () => {
    if (!browserDialog || browserDialog.busy) return
    void runBrowserIntent(browserDialog.intent, true)
  }
  const openBraveDownloadPage = () => {
    if (browserActionLock.current) return
    setBrowserDialog((current) => current ? { ...current, notice: null } : current)
    void invoke('open_brave_download_page').catch((error: unknown) => {
      console.warn('[browser] Não foi possível abrir a página oficial do Brave.', error)
      setBrowserDialog((current) => current
        ? { ...current, notice: 'Não foi possível abrir a página. Acesse o site oficial do Brave pelo navegador.' }
        : current,
      )
    })
  }
  const detail = detailAccountId
    ? accounts.find((account) => account.id === detailAccountId)
    : undefined
  if (isDesktop && coreStatus.error) {
    return (
      <>
        <CustomTitleBar />
        <BootScreen status={coreStatus.error} />
      </>
    )
  }
  if (isDesktop && !coreStatus.ready) {
    return <PreparingShell />
  }
  const content = detail ? (
    <AccountDetail account={detail} onBrowserAction={runAccountBrowserAction} browserActionBusy={browserActionBusy} />
  ) : page === 'Dashboard' ? (
    <Dashboard
      diagnostic={diagnostic}
      onStartReal={startRealAccount}
      addAccountBusy={browserActionBusy}
      addAccountError={addAccountError}
      onRetryAddAccount={canRetryFailedAdd ? retryFailedAdd : null}
      hydrationComplete={hydrationComplete}
    />
  ) : page === 'Automações' ? (
    <Automations />
  ) : page === 'Inventários' ? (
    <Inventory />
  ) : page === 'Mercado' ? (
    <Market />
  ) : page === 'Onde Caçar' ? (
    <WhereToHunt />
  ) : (
    <Settings />
  )
  return (
    <>
      <CustomTitleBar />
      <div className="app-shell">
        <aside>
          <div className="brand">
            <img className="sidebar-brand-art" src={sidebarLogo} alt="Pokeidle Manager" />
          </div>
          <nav>
            {pages.map((item) => (
              <button
                key={item}
                className={item === page && !detail ? 'active' : ''}
                onClick={() => setPage(item)}
              >
                <span
                  className={`sidebar-nav-icon sidebar-nav-icon-${pages.indexOf(item)}`}
                  aria-hidden="true"
                >
                  <SidebarNavIcon page={item} />
                </span>
                <span className="sidebar-nav-label">{item}</span>
              </button>
            ))}
          </nav>
          <div className="sidebar-wallpaper" aria-hidden="true"><img src={sidebarWallpaper} alt="" /></div>
          <footer>
            <span className="pulse" /> Núcleo local
          </footer>
        </aside>
        <div className="content">
          {settings.mockMode && (
            <div className="demo-banner">
              MODO DEMONSTRAÇÃO <span>dados simulados · nenhuma conexão real</span>
            </div>
          )}
          <div className="route-viewport">{content}</div>
        </div>
      </div>
      {browserDialog && (
        <BrowserRequiredDialog
          issue={browserDialog.issue}
          busy={browserDialog.busy}
          notice={browserDialog.notice}
          onDownload={openBraveDownloadPage}
          onRetry={retryBrowserIntent}
          onCancel={() => setBrowserDialog(null)}
        />
      )}
    </>
  )
}
