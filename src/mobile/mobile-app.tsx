import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { formatHuntElapsed } from '../dashboard-time'
import { UiIcon, type UiIconName } from '../components/UiIcon'
import type {
  MobileAccount,
  MobileInventoryPage,
  MobileMarketSummary,
  MobileMarketSummaryPage,
  MobilePokemon,
  MobilePokemonSprite,
  MobileResolvedItemAssets,
  MobileSnapshot,
} from './contract'
import { mobileMockSnapshot } from './mock-snapshot'
import {
  cacheMobileInventoryPage,
  getMobileInventoryPage,
  invalidateMobileInventoryPage,
} from './inventory-cache'

const numberFormat = new Intl.NumberFormat('pt-BR')
const dateFormat = new Intl.DateTimeFormat('pt-BR', { hour: '2-digit', minute: '2-digit' })
const inventoryPageSize = 20
const emptyMarketSummaries: MobileMarketSummary[] = []

type MobilePageName = 'dashboard' | 'automations' | 'inventory' | 'market' | 'settings'
type MobileConnection = 'loading' | 'connected' | 'disconnected'

function useSharedClock() {
  const [now, setNow] = useState(() => performance.now())

  useEffect(() => {
    let timer: number | undefined
    const sync = () => setNow(performance.now())
    const start = () => {
      if (document.visibilityState === 'hidden' || timer !== undefined) return
      sync()
      timer = window.setInterval(sync, 1_000)
    }
    const stop = () => {
      if (timer !== undefined) window.clearInterval(timer)
      timer = undefined
    }
    const onVisibility = () => {
      stop()
      if (document.visibilityState === 'hidden') return
      sync()
      start()
    }

    start()
    document.addEventListener('visibilitychange', onVisibility)
    return () => {
      stop()
      document.removeEventListener('visibilitychange', onVisibility)
    }
  }, [])

  return now
}

function isMobileSnapshot(value: unknown): value is MobileSnapshot {
  if (!value || typeof value !== 'object') return false
  const snapshot = value as Partial<MobileSnapshot>
  return (
    snapshot.schemaVersion === 2 &&
    Number.isSafeInteger(snapshot.revision) &&
    Array.isArray(snapshot.accounts) &&
    !!snapshot.aggregate &&
    !!snapshot.market
  )
}

function useMobileSnapshot(mockMode: boolean) {
  const [snapshot, setSnapshot] = useState<MobileSnapshot | null>(
    mockMode ? mobileMockSnapshot : null,
  )
  const [connection, setConnection] = useState<MobileConnection>(mockMode ? 'connected' : 'loading')
  const current = useRef<MobileSnapshot | null>(mockMode ? mobileMockSnapshot : null)
  const snapshotAt = useRef(performance.now())
  const requestInFlight = useRef(false)

  const apply = useCallback((candidate: unknown) => {
    if (!isMobileSnapshot(candidate)) return false
    if (current.current && candidate.revision <= current.current.revision) return true
    current.current = candidate
    snapshotAt.current = performance.now()
    setSnapshot(candidate)
    return true
  }, [])

  useEffect(() => {
    if (mockMode) return
    let disposed = false
    const getBaseline = async () => {
      if (disposed || requestInFlight.current) return
      requestInFlight.current = true
      try {
        const response = await fetch('/api/v1/mobile/snapshot', { cache: 'no-store' })
        if (!response.ok) throw new Error('snapshot request failed')
        const candidate: unknown = await response.json()
        if (!apply(candidate)) throw new Error('invalid mobile snapshot')
        if (!disposed) setConnection('connected')
      } catch {
        if (!disposed) setConnection('disconnected')
      } finally {
        requestInFlight.current = false
      }
    }

    void getBaseline()
    const events = new EventSource('/api/v1/mobile/events')
    events.onopen = () => {
      void getBaseline()
    }
    events.addEventListener('snapshot', (event) => {
      try {
        if (apply(JSON.parse((event as MessageEvent<string>).data)) && !disposed) {
          setConnection('connected')
        }
      } catch {
        // Keep the last valid snapshot; the next snapshot event replaces it.
      }
    })
    events.onerror = () => {
      if (!disposed) setConnection('disconnected')
    }
    const onVisibility = () => {
      if (document.visibilityState === 'visible') void getBaseline()
    }
    document.addEventListener('visibilitychange', onVisibility)
    return () => {
      disposed = true
      document.removeEventListener('visibilitychange', onVisibility)
      events.close()
    }
  }, [apply, mockMode])

  return { snapshot, connection, snapshotAt: snapshotAt.current }
}

function CurrencyIcon({ kind }: { kind: 'gold' | 'orbs' }) {
  return <UiIcon name={kind === 'gold' ? 'gold' : 'gem'} className={`mobile-currency-icon ${kind === 'gold' ? 'gold-icon' : 'orb-icon'}`} />
}

function MobileGlyph({ kind }: { kind: 'xp' | 'accounts' | 'potion' | 'ball' | 'hunt' | 'safe' }) {
  const iconByKind: Record<typeof kind, UiIconName> = {
    xp: 'xp', accounts: 'accounts', potion: 'potion', ball: 'capture', hunt: 'hunt', safe: 'shield',
  }
  return <UiIcon name={iconByKind[kind]} className={'mobile-glyph glyph-' + kind} />
}

function Metric({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return (
    <div className="mobile-metric">
      <span className="mobile-metric-icon">{icon}</span>
      <span className="mobile-metric-copy">
        <span className="mobile-metric-label">{label}</span>
        <strong>{value}</strong>
      </span>
    </div>
  )
}

function AccountCard({
  account,
  now,
  snapshotAt,
  itemAssetPaths,
  mockMode,
}: {
  account: MobileAccount
  now: number
  snapshotAt: number
  itemAssetPaths: Record<string, string>
  mockMode: boolean
}) {
  const huntTime =
    account.huntElapsedMs === null
      ? null
      : formatHuntElapsed(0, account.huntElapsedMs + Math.max(0, now - snapshotAt))
  const hpPercent =
    account.activePokemon && account.activePokemon.maxHp > 0
      ? Math.min(100, Math.max(0, (account.activePokemon.hp / account.activePokemon.maxHp) * 100))
      : 0
  const ownerLabels = {
    none: 'Sem conexão',
    background: 'Em segundo plano',
    browser: 'Navegador',
    transition: 'Transição',
  }

  return (
    <article className="mobile-account-card" aria-label={'Conta ' + account.displayName}>
      <header className="mobile-account-header">
        <span className="mobile-avatar" aria-hidden="true">
          {account.displayName.slice(0, 1)}
        </span>
        <div className="mobile-account-name">
          <h2>{account.displayName}</h2>
          <span className={'mobile-status status-' + account.status}>
            <span className="status-dot" />
            {account.status === 'online' ? 'Online' : 'Offline'}
          </span>
        </div>
        <span className={'mobile-owner owner-' + account.connectionOwner}>
          {account.connectionOwner === 'browser' && <UiIcon name="browser" />}
          {account.connectionOwner === 'background' && <UiIcon name="background" />}
          {ownerLabels[account.connectionOwner]}
        </span>
      </header>

      <div className="mobile-pokemon-row">
        {account.activePokemon ? (
          <PokemonSpriteView
            pokemon={account.activePokemon}
            className="mobile-pokemon-art"
            size={72}
            disabled={mockMode}
            fallback={
              <span className="mobile-pokemon-placeholder">
                {account.activePokemon.name.slice(0, 1)}
              </span>
            }
          />
        ) : (
          <div className="mobile-pokemon-art" aria-hidden="true">
            <span className="mobile-pokemon-placeholder">·</span>
          </div>
        )}
        <div className="mobile-pokemon-info">
          <div className="mobile-pokemon-title">
            <strong>{account.activePokemon?.name ?? 'Sem Pokémon ativo'}</strong>
            {account.activePokemon && (
              <span>Nv. {numberFormat.format(account.activePokemon.level)}</span>
            )}
          </div>
          {account.activePokemon && (
            <div className="mobile-hp" aria-label={'HP ' + Math.round(hpPercent) + ' por cento'}>
              <div className="mobile-hp-track">
                <span style={{ width: hpPercent + '%' }} />
              </div>
              <small>{Math.round(hpPercent)}%</small>
            </div>
          )}
          <div className="mobile-rate-grid">
            <Metric
              icon={<MobileGlyph kind="xp" />}
              label="XP / hora"
              value={numberFormat.format(account.xpPerHour)}
            />
            <Metric
              icon={<CurrencyIcon kind="gold" />}
              label="Gold / hora"
              value={numberFormat.format(account.goldPerHour)}
            />
            <Metric
              icon={
                account.potion && itemAssetPaths['id:' + account.potion.id] ? (
                  <img
                    className="mobile-equipped-item-sprite"
                    src={itemAssetUrl(itemAssetPaths['id:' + account.potion.id]!)}
                    alt=""
                  />
                ) : (
                  <MobileGlyph kind="potion" />
                )
              }
              label={account.potion?.name ?? 'Poção'}
              value={numberFormat.format(account.potion?.quantity ?? 0)}
            />
            <Metric
              icon={
                account.ball && itemAssetPaths['id:' + account.ball.id] ? (
                  <img
                    className="mobile-equipped-item-sprite"
                    src={itemAssetUrl(itemAssetPaths['id:' + account.ball.id]!)}
                    alt=""
                  />
                ) : (
                  <MobileGlyph kind="ball" />
                )
              }
              label={account.ball?.name ?? 'Pokébola'}
              value={numberFormat.format(account.ball?.quantity ?? 0)}
            />
          </div>
        </div>
      </div>

      <footer className="mobile-account-footer">
        <div className="mobile-hunt">
          <span className="hunt-mark">
            <MobileGlyph kind="hunt" />
          </span>
          <span className="hunt-label">Hunt</span>
          <strong>{account.huntName ?? 'Sem hunt'}</strong>
          {huntTime && <time className="hunt-timer"><UiIcon name="clock" />{huntTime}</time>}
        </div>
        <div className="mobile-wallet" aria-label="Carteira">
          <span>
            <CurrencyIcon kind="gold" />
            {numberFormat.format(account.gold ?? 0)}
          </span>
          <span>
            <CurrencyIcon kind="orbs" />
            {numberFormat.format(account.orbs ?? 0)}
          </span>
        </div>
      </footer>
    </article>
  )
}

function PageHeading({
  id,
  eyebrow,
  title,
  subtitle,
}: {
  id: string
  eyebrow: string
  title: string
  subtitle: string
}) {
  return (
    <header className="mobile-page-heading" aria-labelledby={id}>
      <div
        className="mobile-page-scenery"
        aria-hidden="true"
        style={{
          background:
            'radial-gradient(ellipse at 78% 30%, #d39a5425, transparent 38%), linear-gradient(135deg, #30232d, #1e1922 64%, #2b2029)',
        }}
      />
      <div
        className="mobile-page-titleboard"
        style={{
          display: 'grid',
          placeItems: 'center',
          border: '1px solid #bd8150',
          borderRadius: '10px 4px 10px 4px',
          background: 'linear-gradient(125deg, #49313a, #271f2a 72%)',
          boxShadow: 'inset 0 0 0 3px #d39a5417, 0 3px #171119',
        }}
      >
        <h1 id={id}>{title}</h1>
      </div>
      <div className="mobile-page-heading-copy">
        <p className="mobile-eyebrow">{eyebrow}</p>
        <p className="mobile-subtitle">{subtitle}</p>
      </div>
    </header>
  )
}

function DashboardPage({
  snapshot,
  accounts,
  now,
  snapshotAt,
  mockMode,
  connection,
}: {
  snapshot: MobileSnapshot | null
  accounts: MobileAccount[]
  now: number
  snapshotAt: number
  mockMode: boolean
  connection: MobileConnection
}) {
  const aggregate = snapshot?.aggregate
  const resourceReferences = useMemo(
    () =>
      accounts.flatMap((account) =>
        [account.potion?.id, account.ball?.id].flatMap((id) =>
          typeof id === 'number' && id > 0 ? [{ id }] : [],
        ),
      ),
    [accounts],
  )
  const itemAssetPaths = useMobileItemAssetPaths(resourceReferences, mockMode)
  return (
    <section className="mobile-page" aria-labelledby="mobile-title">
      <PageHeading
        id="mobile-title"
        eyebrow="VISÃO GERAL"
        title="Dashboard"
        subtitle="Resumo das contas em um só lugar"
      />
      <div className={'mobile-live-indicator connection-' + connection} aria-live="polite">
        <span />
        {mockMode
          ? 'Dados de demonstração'
          : connection === 'connected'
            ? 'Conectado ao Manager'
            : connection === 'loading'
              ? 'Conectando ao Manager…'
              : 'Desconectado · dados mantidos'}
      </div>
      {aggregate && (
        <section className="mobile-summary" aria-label="Resumo geral">
          <Metric
            icon={<MobileGlyph kind="xp" />}
            label="XP / hora total"
            value={numberFormat.format(aggregate.xpPerHourTotal)}
          />
          <Metric
            icon={<CurrencyIcon kind="gold" />}
            label="Gold / hora total"
            value={numberFormat.format(aggregate.goldPerHourTotal)}
          />
          <Metric
            icon={<MobileGlyph kind="accounts" />}
            label="Contas online"
            value={aggregate.onlineAccounts + '/' + aggregate.totalAccounts}
          />
          <Metric
            icon={<CurrencyIcon kind="gold" />}
            label="Gold total"
            value={numberFormat.format(aggregate.goldTotal)}
          />
          <Metric
            icon={<CurrencyIcon kind="orbs" />}
            label="Gemas totais"
            value={numberFormat.format(aggregate.orbsTotal)}
          />
        </section>
      )}
      <div className="mobile-section-heading">
        <div>
          <p className="mobile-eyebrow">SUAS CONTAS</p>
          <h2>Atividade atual</h2>
        </div>
        <span>{accounts.length} contas</span>
      </div>
      <section className="mobile-account-list" aria-label="Contas">
        {accounts.map((account) => (
          <AccountCard
            key={account.id}
            account={account}
            now={now}
            snapshotAt={snapshotAt}
            itemAssetPaths={itemAssetPaths}
            mockMode={mockMode}
          />
        ))}
      </section>
      {connection === 'loading' && accounts.length === 0 && (
        <p className="mobile-empty">Aguardando os dados locais do Manager…</p>
      )}
      {connection === 'disconnected' && accounts.length === 0 && (
        <p className="mobile-empty">
          Não foi possível conectar ao Manager. Mantenha-o aberto e tente novamente.
        </p>
      )}
      {mockMode && (
        <p className="mobile-disclaimer">
          Prévia com informações fictícias. Remova <code>?mode=mock</code> para usar dados reais.
        </p>
      )}
    </section>
  )
}

const automationLabels: Array<[keyof MobileAccount['automations'], string]> = [
  ['autoPotion', 'Poções automáticas'],
  ['autoRevive', 'Reviver automaticamente'],
  ['autoSaleLoot', 'Venda automática de loot'],
  ['autoReturnHunt', 'Retornar à hunt'],
  ['autoLockShiny', 'Proteger Pokémon shiny'],
  ['autoLockNota9', 'Proteger nota alta'],
  ['autoLockP5', 'Proteger P5'],
]

function AutomationPage({
  accounts,
  now,
  snapshotAt,
}: {
  accounts: MobileAccount[]
  now: number
  snapshotAt: number
}) {
  return (
    <section className="mobile-page" aria-labelledby="automation-title">
      <PageHeading
        id="automation-title"
        eyebrow="MONITORAMENTO"
        title="Automações"
        subtitle="Estado atual por conta · somente leitura"
      />
      <div className="mobile-status-strip">
        <span className="status-shield">
          <MobileGlyph kind="safe" />
        </span>
        <span>Visualização segura</span>
        <strong>{accounts.length} contas</strong>
      </div>
      <div className="mobile-automation-list">
        {accounts.map((account) => (
          <article className="mobile-panel mobile-automation-card" key={account.id}>
            <header className="mobile-panel-heading">
              <div>
                <span className={'status-pill status-' + account.status}>
                  {account.status === 'online' ? 'Online' : 'Offline'}
                </span>
                <h2>{account.displayName}</h2>
              </div>
              <span className="owner-pill">
                {account.connectionOwner === 'browser' && <UiIcon name="browser" />}
                {account.connectionOwner === 'background' && <UiIcon name="background" />}
                {account.connectionOwner === 'background'
                  ? 'Segundo plano'
                  : account.connectionOwner === 'browser'
                    ? 'Navegador'
                    : account.connectionOwner === 'transition'
                      ? 'Transição'
                      : 'Sem conexão'}
              </span>
            </header>
            <div className="mobile-automation-hunt">
              <span>Hunt atual</span>
              <strong>{account.huntName ?? 'Sem hunt'}</strong>
              {account.huntElapsedMs !== null && (
                <time>
                  {formatHuntElapsed(0, account.huntElapsedMs + Math.max(0, now - snapshotAt))}
                </time>
              )}
            </div>
            <div className="mobile-automation-metrics">
              {automationLabels.map(([key, label]) => (
                <div className="automation-state" key={key}>
                  <span
                    className={'automation-light' + (account.automations[key] ? ' is-on' : '')}
                  />
                  <span>{label}</span>
                  <strong>{account.automations[key] ? 'Ativa' : 'Inativa'}</strong>
                </div>
              ))}
              <div className="automation-state">
                <span
                  className={
                    'automation-light' + (account.automations.captureMode !== 'off' ? ' is-on' : '')
                  }
                />
                <span>Captura</span>
                <strong>
                  {account.automations.captureMode === 'continuous'
                    ? 'Contínua'
                    : account.automations.captureMode === 'until_capture'
                      ? 'Até capturar'
                      : 'Desativada'}
                </strong>
              </div>
              {account.automations.captureQueueLen > 0 && (
                <div className="automation-note">
                  Fila de captura: {numberFormat.format(account.automations.captureQueueLen)}
                </div>
              )}
            </div>
          </article>
        ))}
        {accounts.length === 0 && <p className="mobile-empty">Nenhuma conta disponível.</p>}
      </div>
      <p className="mobile-footnote">
        As automações permanecem no controle do Manager. Esta tela não permite alterá-las.
      </p>
    </section>
  )
}

const mobilePokemonSpriteRequests = new Map<string, Promise<MobilePokemonSprite | null>>()
const mobilePokemonSpriteCacheLimit = 256

function cacheMobilePokemonSprite(key: string, request: Promise<MobilePokemonSprite | null>) {
  mobilePokemonSpriteRequests.delete(key)
  mobilePokemonSpriteRequests.set(key, request)
  while (mobilePokemonSpriteRequests.size > mobilePokemonSpriteCacheLimit) {
    const oldest = mobilePokemonSpriteRequests.keys().next().value as string | undefined
    if (!oldest) break
    mobilePokemonSpriteRequests.delete(oldest)
  }
}

type PokemonSpriteReference = Pick<MobilePokemon, 'looktype' | 'lookShiny' | 'shiny'>

function useMobilePokemonSprite(pokemon: PokemonSpriteReference, disabled: boolean) {
  const looktypes = [
    ...new Set(
      (pokemon.shiny ? [pokemon.lookShiny, pokemon.looktype] : [pokemon.looktype]).filter(
        (value): value is number => typeof value === 'number' && value > 0,
      ),
    ),
  ]
  const key = looktypes.join(',')
  const [sprite, setSprite] = useState<MobilePokemonSprite | null>(null)
  useEffect(() => {
    let active = true
    if (disabled || !key) {
      setSprite(null)
      return () => {
        active = false
      }
    }
    let request = mobilePokemonSpriteRequests.get(key)
    if (request) {
      mobilePokemonSpriteRequests.delete(key)
      mobilePokemonSpriteRequests.set(key, request)
    }
    if (!request) {
      request = fetch('/api/v1/mobile/pokemon-sprite?looktypes=' + encodeURIComponent(key), {
        cache: 'force-cache',
      })
        .then(async (response) =>
          response.ok ? ((await response.json()) as MobilePokemonSprite) : null,
        )
        .catch(() => null)
      cacheMobilePokemonSprite(key, request)
    }
    void request.then((result) => {
      if (active) setSprite(result)
    })
    return () => {
      active = false
    }
  }, [disabled, key])
  return sprite
}

const mobileItemAssetPaths = new Map<string, string | null>()
const mobileItemAssetCacheLimit = 512

function itemAssetKey(reference: { id?: number; name?: string }) {
  return reference.id !== undefined
    ? 'id:' + reference.id
    : 'name:' + (reference.name ?? '').trim().toLocaleLowerCase()
}

function useMobileItemAssetPaths(
  references: Array<{ id?: number; name?: string }>,
  disabled: boolean,
) {
  const uniqueReferences = useMemo(() => {
    const unique = new Map<string, { id?: number; name?: string }>()
    for (const reference of references) {
      if (reference.id !== undefined && reference.id > 0) {
        unique.set(itemAssetKey(reference), reference)
      } else if (reference.name?.trim()) {
        unique.set(itemAssetKey(reference), reference)
      }
    }
    return [...unique.entries()]
  }, [references])
  const referenceKey = uniqueReferences.map(([key]) => key).join('|')
  const [paths, setPaths] = useState<Record<string, string>>({})

  useEffect(() => {
    let active = true
    if (disabled || uniqueReferences.length === 0) {
      setPaths((current) => (Object.keys(current).length ? {} : current))
      return () => {
        active = false
      }
    }
    const missing = uniqueReferences.filter(([key]) => !mobileItemAssetPaths.has(key))
    const applyCached = () => {
      if (!active) return
      const next = Object.fromEntries(
        uniqueReferences.flatMap(([key]) => {
          const path = mobileItemAssetPaths.get(key)
          return path ? [[key, path]] : []
        }),
      )
      setPaths((current) =>
        Object.keys(current).length === Object.keys(next).length &&
        Object.entries(next).every(([key, path]) => current[key] === path)
          ? current
          : next,
      )
    }
    if (missing.length === 0) {
      applyCached()
      return () => {
        active = false
      }
    }
    const query = new URLSearchParams()
    const ids = missing.flatMap(([, reference]) =>
      reference.id === undefined ? [] : [reference.id],
    )
    if (ids.length) query.set('ids', ids.join(','))
    for (const [, reference] of missing) {
      if (reference.name?.trim()) query.append('name', reference.name.trim())
    }
    void fetch('/api/v1/mobile/item-assets?' + query.toString(), { cache: 'force-cache' })
      .then(async (response) => {
        if (!response.ok) throw new Error('item assets request failed')
        return (await response.json()) as MobileResolvedItemAssets
      })
      .then((result) => {
        for (const item of result.items) {
          const path = item.assetPath
          if (item.id !== null) mobileItemAssetPaths.set('id:' + item.id, path)
          if (item.name)
            mobileItemAssetPaths.set('name:' + item.name.trim().toLocaleLowerCase(), path)
        }
        while (mobileItemAssetPaths.size > mobileItemAssetCacheLimit) {
          const oldest = mobileItemAssetPaths.keys().next().value as string | undefined
          if (!oldest) break
          mobileItemAssetPaths.delete(oldest)
        }
        applyCached()
      })
      .catch(() => applyCached())
    return () => {
      active = false
    }
  }, [disabled, referenceKey, uniqueReferences])

  return paths
}

function itemAssetUrl(assetPath: string) {
  return '/api/v1/mobile/asset?path=' + encodeURIComponent(assetPath)
}

function PokemonSpriteView({
  pokemon,
  className,
  size,
  disabled = false,
  lazy = false,
  fallback,
}: {
  pokemon: PokemonSpriteReference
  className: string
  size: number
  disabled?: boolean
  lazy?: boolean
  fallback?: ReactNode
}) {
  const host = useRef<HTMLSpanElement>(null)
  const [nearViewport, setNearViewport] = useState(!lazy)
  useEffect(() => {
    if (!lazy) {
      setNearViewport(true)
      return
    }
    const element = host.current
    if (!element || typeof IntersectionObserver === 'undefined') {
      setNearViewport(true)
      return
    }
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setNearViewport(true)
          observer.disconnect()
        }
      },
      { rootMargin: '120px' },
    )
    observer.observe(element)
    return () => observer.disconnect()
  }, [lazy])
  const sprite = useMobilePokemonSprite(pokemon, disabled || !nearViewport)
  const scale = sprite ? Math.min(size / sprite.width, size / sprite.height) : 1
  return (
    <span ref={host} className={className} aria-hidden="true">
      {sprite ? (
        <span
          className="mobile-sprite-frame"
          style={{
            width: sprite.width * scale,
            height: sprite.height * scale,
            backgroundImage: `url("${itemAssetUrl(sprite.assetPath)}")`,
            backgroundSize: `${sprite.pageWidth * scale}px ${sprite.pageHeight * scale}px`,
            backgroundPosition: `-${sprite.x * scale}px -${sprite.y * scale}px`,
          }}
        />
      ) : (
        (fallback ?? <UiIcon name="pokemon" className="pokemon-sprite-fallback" />)
      )}
    </span>
  )
}

function InventoryPokemonRow({
  pokemon,
  accountName,
  mockMode,
}: {
  pokemon: MobileInventoryPage['pokemon'][number]
  accountName: string
  mockMode: boolean
}) {
  return (
    <article className="mobile-inventory-row pokemon-inventory-row" key={pokemon.id}>
      <PokemonSpriteView
        pokemon={pokemon}
        className="inventory-pokemon-sprite"
        size={54}
        disabled={mockMode}
        lazy
      />
      <span className="inventory-row-name">
        {pokemon.name}
        {pokemon.shiny && <em className="shiny-tag">Shiny</em>}
        <small>
          {accountName} · Nv. {numberFormat.format(pokemon.level)}
          {pokemon.types.length ? ' · ' + pokemon.types.join(' / ') : ''}
        </small>
      </span>
      <span className="pokemon-quality">
        {pokemon.quality === null ? '—' : numberFormat.format(pokemon.quality)}
      </span>
      <small className="pokemon-inventory-stats">
        IV {pokemon.ivTotal ?? '—'} · P{pokemon.power ?? '—'} · Nota{' '}
        {pokemon.note?.toFixed(2) ?? '—'}
      </small>
    </article>
  )
}

function InventoryPage({ accounts, mockMode }: { accounts: MobileAccount[]; mockMode: boolean }) {
  const [accountId, setAccountId] = useState('all')
  const [kind, setKind] = useState<'items' | 'pokemon'>('items')
  const [category, setCategory] = useState('all')
  const [pokemonType, setPokemonType] = useState('all')
  const [minimumIv, setMinimumIv] = useState('')
  const [order, setOrder] = useState('level')
  const [search, setSearch] = useState('')
  const [pageIndex, setPageIndex] = useState(0)
  const [page, setPage] = useState<MobileInventoryPage | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(false)
  const [refreshVersion, setRefreshVersion] = useState(0)
  const debouncedSearch = useDebouncedValue(search.trim(), 250)
  const debouncedMinimumIv = useDebouncedValue(minimumIv, 250)
  const selectedAccountId = accounts.some((account) => account.id === accountId)
    ? accountId
    : accountId === 'all'
      ? 'all'
      : (accounts[0]?.id ?? '')
  const accountIdsKey = accounts.map((account) => account.id).join(',')
  const selectedAccountIds = useMemo(
    () =>
      selectedAccountId === 'all'
        ? accountIdsKey
          ? accountIdsKey.split(',')
          : []
        : selectedAccountId
          ? [selectedAccountId]
          : [],
    [accountIdsKey, selectedAccountId],
  )
  const requestQuery = new URLSearchParams({
    kind,
    offset: String(pageIndex * inventoryPageSize),
    limit: String(inventoryPageSize),
  })
  if (debouncedSearch) requestQuery.set('q', debouncedSearch)
  if (kind === 'items' && category !== 'all') {
    requestQuery.set(
      'category',
      (
        { potions: 'potion', balls: 'ball', stones: 'stone', other: 'other' } as Record<
          string,
          string
        >
      )[category],
    )
  }
  if (kind === 'pokemon') {
    if (pokemonType !== 'all') requestQuery.set('type', pokemonType)
    if (debouncedMinimumIv !== '')
      requestQuery.set('minIv', String(Math.max(0, Number(debouncedMinimumIv) || 0)))
    requestQuery.set('order', order)
  }
  const pageQuery = requestQuery.toString()
  const cacheKey = pageQuery + '&accountId=' + selectedAccountId

  useEffect(() => {
    setPageIndex(0)
  }, [kind, category, pokemonType, debouncedMinimumIv, order, debouncedSearch, selectedAccountId])

  useEffect(() => {
    if (mockMode || selectedAccountIds.length === 0) return
    const abort = new AbortController()
    let active = true
    const cached = getMobileInventoryPage(cacheKey)
    if (cached) {
      setPage(cached)
      setLoading(false)
      setError(false)
      return
    }
    const load = async () => {
      setLoading(true)
      setError(false)
      try {
        const results = await Promise.all(
          selectedAccountIds.map(async (id) => {
            const query = new URLSearchParams(pageQuery)
            query.set('accountId', id)
            const response = await fetch('/api/v1/mobile/inventory?' + query.toString(), {
              cache: 'no-store',
              signal: abort.signal,
            })
            if (!response.ok) throw new Error('inventory request failed')
            const result: MobileInventoryPage = await response.json()
            if (result.accountId !== id || result.kind !== kind)
              throw new Error('inventory response mismatch')
            return result
          }),
        )
        if (active && results.length) {
          const result: MobileInventoryPage & { maxAccountTotal: number } = {
            accountId: selectedAccountId,
            kind,
            offset: pageIndex * inventoryPageSize,
            limit: inventoryPageSize,
            total: results.reduce((sum, page) => sum + page.total, 0),
            maxAccountTotal: Math.max(...results.map((page) => page.total)),
            items: results.flatMap((page) =>
              page.items.map((item) => ({ ...item, accountId: page.accountId })),
            ),
            pokemon: results.flatMap((page) =>
              page.pokemon.map((pokemon) => ({ ...pokemon, accountId: page.accountId })),
            ),
            types: [...new Set(results.flatMap((page) => page.types))].sort(),
          }
          setPage(result)
          cacheMobileInventoryPage(cacheKey, result)
        }
      } catch {
        if (active && !abort.signal.aborted) setError(true)
      } finally {
        if (active) setLoading(false)
      }
    }
    void load()
    return () => {
      active = false
      abort.abort()
    }
  }, [
    cacheKey,
    kind,
    mockMode,
    pageQuery,
    refreshVersion,
    selectedAccountId,
    selectedAccountIds,
    pageIndex,
  ])

  const changeKind = (next: 'items' | 'pokemon') => {
    setKind(next)
    setCategory('all')
    setPokemonType('all')
    setMinimumIv('')
    setOrder('level')
    setPageIndex(0)
  }
  const totalPages = page
    ? Math.max(
        1,
        Math.ceil(
          ((page as MobileInventoryPage & { maxAccountTotal?: number }).maxAccountTotal ??
            page.total) / inventoryPageSize,
        ),
      )
    : 1

  return (
    <section className="mobile-page" aria-labelledby="inventory-title">
      <PageHeading
        id="inventory-title"
        eyebrow="VISUALIZAÇÃO"
        title="Inventários"
        subtitle="Itens e Pokémon armazenados por conta"
      />
      <div className="mobile-inventory-controls mobile-panel">
        <label className="mobile-field">
          <span>Conta</span>
          <select
            value={selectedAccountId}
            onChange={(event) => {
              setPage(null)
              setAccountId(event.target.value)
            }}
            aria-label="Conta do inventário"
          >
            <option value="all">Todas as contas</option>
            {accounts.map((account) => (
              <option key={account.id} value={account.id}>
                {account.displayName}
              </option>
            ))}
          </select>
        </label>
        <div className="mobile-segmented" aria-label="Tipo de inventário">
          <button type="button" aria-pressed={kind === 'items'} onClick={() => changeKind('items')}>
            Itens
          </button>
          <button
            type="button"
            aria-pressed={kind === 'pokemon'}
            onClick={() => changeKind('pokemon')}
          >
            Pokémon
          </button>
        </div>
        <label className="mobile-search">
          <span className="sr-only">Buscar no inventário</span>
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={kind === 'items' ? 'Buscar item…' : 'Buscar Pokémon…'}
          />
        </label>
        {kind === 'items' && (
          <div className="mobile-chip-row" aria-label="Categoria de itens">
            {['all', 'potions', 'balls', 'stones', 'other'].map((value) => (
              <button
                className="mobile-chip"
                type="button"
                aria-pressed={category === value}
                key={value}
                onClick={() => {
                  setCategory(value)
                  setPageIndex(0)
                }}
              >
                {
                  (
                    {
                      all: 'Todos',
                      potions: 'Potions',
                      balls: 'Balls',
                      stones: 'Stones',
                      other: 'Outros',
                    } as Record<string, string>
                  )[value]
                }
              </button>
            ))}
          </div>
        )}
        {kind === 'pokemon' && (
          <div className="mobile-pokemon-filters">
            <label className="mobile-field">
              <span>Tipo</span>
              <select
                aria-label="Tipo de Pokémon"
                value={pokemonType}
                onChange={(event) => {
                  setPokemonType(event.target.value)
                  setPageIndex(0)
                }}
              >
                <option value="all">Todos os tipos</option>
                {page?.types.map((type) => (
                  <option key={type} value={type}>
                    {type}
                  </option>
                ))}
              </select>
            </label>
            <label className="mobile-field">
              <span>IV mínimo</span>
              <input
                aria-label="IV mínimo"
                type="number"
                inputMode="numeric"
                min="0"
                max="1000"
                placeholder="Qualquer"
                value={minimumIv}
                onChange={(event) => {
                  setMinimumIv(event.target.value)
                  setPageIndex(0)
                }}
              />
            </label>
            <label className="mobile-field">
              <span>Ordenar por</span>
              <select
                aria-label="Ordenar Pokémon"
                value={order}
                onChange={(event) => {
                  setOrder(event.target.value)
                  setPageIndex(0)
                }}
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
        )}
      </div>
      {mockMode ? (
        <p className="mobile-empty">Inventário real ficará disponível ao conectar ao Manager.</p>
      ) : loading && !page ? (
        <p className="mobile-empty">Carregando uma página do inventário…</p>
      ) : error ? (
        <div className="mobile-empty">
          Não foi possível carregar. Verifique a conexão e tente mudar de conta ou filtro.
        </div>
      ) : (
        page && (
          <>
            <div className="mobile-list-summary">
              <span>
                {numberFormat.format(page.total)} {kind === 'items' ? 'tipos de item' : 'Pokémon'}
              </span>
              <span>
                Página {pageIndex + 1} de {totalPages}
              </span>
              <button
                type="button"
                onClick={() => {
                  invalidateMobileInventoryPage(cacheKey)
                  setRefreshVersion((value) => value + 1)
                }}
              >
                Atualizar
              </button>
            </div>
            {kind === 'items' ? (
              <div className="mobile-inventory-list" aria-live="polite">
                {page.items.map((item) => (
                  <article className="mobile-inventory-row" key={(item.accountId ?? '') + item.id}>
                    <span
                      className={'inventory-item-mark category-' + item.category}
                      aria-hidden="true"
                    >
                      {item.assetPath ? (
                        <img
                          src={'/api/v1/mobile/asset?path=' + encodeURIComponent(item.assetPath)}
                          alt=""
                          loading="lazy"
                        />
                      ) : item.category === 'ball' ? (
                        <UiIcon name="capture" />
                      ) : item.category === 'potion' ? (
                        <UiIcon name="potion" />
                      ) : item.category === 'stone' ? (
                        <UiIcon name="gem" />
                      ) : (
                        <UiIcon name="inventory" />
                      )}
                    </span>
                    <span className="inventory-row-name">
                      {item.name}
                      <small>
                        {item.accountId && (
                          <>
                            {accounts.find((account) => account.id === item.accountId)
                              ?.displayName ?? item.accountId}{' '}
                            ·{' '}
                          </>
                        )}
                        {item.category === 'ball'
                          ? 'Ball'
                          : item.category === 'potion'
                            ? 'Potion'
                            : item.category === 'stone'
                              ? 'Stone'
                              : 'Item'}
                      </small>
                    </span>
                    <strong>{numberFormat.format(item.quantity)}</strong>
                  </article>
                ))}
                {page.items.length === 0 && (
                  <p className="mobile-empty inline-empty">Nenhum item encontrado.</p>
                )}
              </div>
            ) : (
              <div className="mobile-inventory-list" aria-live="polite">
                {page.pokemon.map((pokemon) => (
                  <InventoryPokemonRow
                    key={(pokemon.accountId ?? '') + pokemon.id}
                    pokemon={pokemon}
                    accountName={
                      accounts.find((account) => account.id === pokemon.accountId)?.displayName ??
                      pokemon.accountId ??
                      ''
                    }
                    mockMode={mockMode}
                  />
                ))}
                {page.pokemon.length === 0 && (
                  <p className="mobile-empty inline-empty">Nenhum Pokémon encontrado.</p>
                )}
              </div>
            )}
            {((page as MobileInventoryPage & { maxAccountTotal?: number }).maxAccountTotal ??
              page.total) > inventoryPageSize && (
              <nav className="mobile-pagination" aria-label="Paginação do inventário">
                <button
                  type="button"
                  onClick={() => setPageIndex((value) => Math.max(0, value - 1))}
                  disabled={pageIndex === 0}
                >
                  Anterior
                </button>
                <span>
                  {numberFormat.format(pageIndex * inventoryPageSize + 1)}–
                  {numberFormat.format(
                    Math.min(
                      (pageIndex + 1) * inventoryPageSize,
                      (page as MobileInventoryPage & { maxAccountTotal?: number })
                        .maxAccountTotal ?? page.total,
                    ),
                  )}{' '}
                  por conta
                </span>
                <button
                  type="button"
                  onClick={() => setPageIndex((value) => Math.min(totalPages - 1, value + 1))}
                  disabled={pageIndex + 1 >= totalPages}
                >
                  Próxima
                </button>
              </nav>
            )}
          </>
        )
      )}
      {loading && page && <p className="mobile-refresh-note">Atualizando página…</p>}
      <p className="mobile-footnote">Só a página solicitada é carregada; nada é enviado ao jogo.</p>
    </section>
  )
}

function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delayMs)
    return () => window.clearTimeout(timer)
  }, [delayMs, value])
  return debounced
}

type MarketFilter = 'all' | 'gold' | 'gems'

function MarketPage({
  snapshot,
  mockMode,
}: {
  snapshot: MobileSnapshot | null
  mockMode: boolean
}) {
  const market = snapshot?.market
  const [filter, setFilter] = useState<MarketFilter>('all')
  const [tab, setTab] = useState<'offers' | 'top' | 'recent'>('offers')
  const [search, setSearch] = useState('')
  const [category, setCategory] = useState('all')
  const [pageIndex, setPageIndex] = useState(0)
  const [summaryResult, setSummaryResult] = useState<{
    query: string
    page: MobileMarketSummaryPage
  } | null>(null)
  const [summaryLoading, setSummaryLoading] = useState(false)
  const [summaryError, setSummaryError] = useState(false)
  const debouncedSearch = useDebouncedValue(search.trim(), 180)
  const marketPageSize = 50
  const mockSummaries = useMemo(
    () =>
      (market?.summaries ?? []).filter((item) => {
        const nameMatches =
          !debouncedSearch ||
          item.name.toLocaleLowerCase().includes(debouncedSearch.toLocaleLowerCase())
        const currencyMatches =
          filter === 'all' || (filter === 'gold' ? item.minGold !== null : item.minOrb !== null)
        const categoryMatches = category === 'all' || item.category === category
        return nameMatches && currencyMatches && categoryMatches
      }),
    [market?.summaries, filter, category, debouncedSearch],
  )
  const marketQuery = new URLSearchParams({
    offset: String(pageIndex * marketPageSize),
    limit: String(marketPageSize),
    currency: filter,
  })
  if (debouncedSearch) marketQuery.set('q', debouncedSearch)
  if (category !== 'all') marketQuery.set('category', category)
  const marketQueryString = marketQuery.toString()

  useEffect(() => {
    setPageIndex(0)
  }, [filter, category, debouncedSearch])

  useEffect(() => {
    if (mockMode || tab !== 'offers') return
    const abort = new AbortController()
    let active = true
    const load = async () => {
      setSummaryLoading(true)
      setSummaryError(false)
      try {
        const response = await fetch('/api/v1/mobile/market/summaries?' + marketQueryString, {
          cache: 'no-store',
          signal: abort.signal,
        })
        if (!response.ok) throw new Error('market summaries request failed')
        const result: MobileMarketSummaryPage = await response.json()
        if (active) setSummaryResult({ query: marketQueryString, page: result })
      } catch {
        if (active && !abort.signal.aborted) setSummaryError(true)
      } finally {
        if (active) setSummaryLoading(false)
      }
    }
    void load()
    return () => {
      active = false
      abort.abort()
    }
  }, [marketQueryString, mockMode, market?.lastUpdatedAt, tab])

  const currentSummaryPage = summaryResult?.query === marketQueryString ? summaryResult.page : null
  const summaries = useMemo(
    () => (mockMode ? mockSummaries : (currentSummaryPage?.items ?? emptyMarketSummaries)),
    [currentSummaryPage, mockMode, mockSummaries],
  )
  const categories = mockMode
    ? [...new Set((market?.summaries ?? []).map((item) => item.category))].sort()
    : (currentSummaryPage?.categories ?? [])
  const totalOffers = mockMode ? mockSummaries.length : (currentSummaryPage?.total ?? 0)
  const totalOfferPages = Math.max(1, Math.ceil(totalOffers / marketPageSize))
  const topSales = useMemo(
    () =>
      (market?.topItemSales ?? []).filter((item) => {
        const nameMatches =
          !debouncedSearch ||
          item.itemName.toLocaleLowerCase().includes(debouncedSearch.toLocaleLowerCase())
        return item.currencyGroup === filter && nameMatches
      }),
    [market?.topItemSales, filter, debouncedSearch],
  )
  const recentSales = useMemo(
    () =>
      (market?.recentTransactions ?? []).filter((sale) => {
        const nameMatches =
          !debouncedSearch ||
          sale.name.toLocaleLowerCase().includes(debouncedSearch.toLocaleLowerCase())
        const currencyMatches =
          filter === 'all' ||
          (filter === 'gold' ? sale.currency === 'gold' : sale.currency === 'orb')
        return nameMatches && currencyMatches
      }),
    [market?.recentTransactions, filter, debouncedSearch],
  )
  const marketAssetReferences = useMemo(() => {
    if (tab === 'offers') return summaries.map((item) => ({ id: item.itemId }))
    if (tab === 'top') return topSales.map((sale) => ({ name: sale.itemName }))
    return recentSales.filter((sale) => sale.kind === 'item').map((sale) => ({ name: sale.name }))
  }, [recentSales, summaries, tab, topSales])
  const marketItemAssets = useMobileItemAssetPaths(marketAssetReferences, mockMode)

  return (
    <section className="mobile-page" aria-labelledby="market-title">
      <PageHeading
        id="market-title"
        eyebrow="SOMENTE LEITURA"
        title="Mercado"
        subtitle="Resumo e histórico do mercado"
      />
      <article className="mobile-panel market-status-panel">
        <div>
          <span className="mobile-eyebrow">LEITOR DE MERCADO</span>
          <strong className="market-reader-status">
            {market ? humanizeStatus(market.readerStatus) : 'Aguardando leitura'}
          </strong>
        </div>
        <div className="market-status-meta">
          <span>
            Regras ativas: <strong>{market?.enabledRuleCount ?? 0}</strong>/
            {market?.totalRuleCount ?? 0}
          </span>
          {market?.lastUpdatedAt && (
            <time>Atualizado {dateFormat.format(market.lastUpdatedAt)}</time>
          )}
        </div>
      </article>
      <div className="mobile-segmented market-tabs" role="tablist" aria-label="Seção do mercado">
        {(
          [
            ['offers', 'Ofertas'],
            ['top', 'Mais vendidos'],
            ['recent', 'Últimas vendas'],
          ] as const
        ).map(([key, label]) => (
          <button
            role="tab"
            aria-selected={tab === key}
            type="button"
            key={key}
            onClick={() => setTab(key)}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="mobile-market-controls">
        <label className="mobile-search">
          <span className="sr-only">Buscar no mercado</span>
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Buscar item…"
          />
        </label>
        <div className="mobile-chip-row" aria-label="Filtro de moeda">
          {(
            [
              ['all', 'Todas'],
              ['gold', 'Gold'],
              ['gems', 'Gemas'],
            ] as const
          ).map(([key, label]) => (
            <button
              className="mobile-chip"
              type="button"
              aria-pressed={filter === key}
              key={key}
              onClick={() => setFilter(key)}
            >
              {label}
            </button>
          ))}
        </div>
        {tab === 'offers' && categories.length > 0 && (
          <div className="mobile-chip-row" aria-label="Categoria do mercado">
            <button
              className="mobile-chip"
              type="button"
              aria-pressed={category === 'all'}
              onClick={() => setCategory('all')}
            >
              Todas as categorias
            </button>
            {categories.map((value) => (
              <button
                className="mobile-chip"
                type="button"
                aria-pressed={category === value}
                key={value}
                onClick={() => setCategory(value)}
              >
                {humanizeCategory(value)}
              </button>
            ))}
          </div>
        )}
      </div>
      {tab === 'offers' && (
        <div className="mobile-market-list" aria-live="polite">
          <div className="market-column-head">
            <span aria-hidden="true" />
            <span>Item</span>
            <span>Ofertas</span>
            <span>Unidades</span>
            <span>Menor preço</span>
          </div>
          {summaries.map((item) => (
            <OfferRow
              item={item}
              filter={filter}
              assetPath={marketItemAssets['id:' + item.itemId]}
              key={item.itemId}
            />
          ))}
          {summaryLoading && summaries.length === 0 && (
            <p className="mobile-empty">Carregando ofertas…</p>
          )}
          {summaryError && summaries.length === 0 && (
            <p className="mobile-empty">
              Não foi possível carregar as ofertas. Verifique a conexão e tente novamente.
            </p>
          )}
          {!summaryLoading && !summaryError && summaries.length === 0 && (
            <p className="mobile-empty">Aguardando dados ou nenhum item corresponde ao filtro.</p>
          )}
          {!mockMode && summaries.length > 0 && (
            <nav className="mobile-pagination" aria-label="Paginação das ofertas do mercado">
              <button
                type="button"
                onClick={() => setPageIndex((value) => Math.max(0, value - 1))}
                disabled={pageIndex === 0 || summaryLoading}
              >
                Anterior
              </button>
              <span>
                {numberFormat.format(totalOffers)} ofertas · Página {pageIndex + 1}/
                {totalOfferPages}
              </span>
              <button
                type="button"
                onClick={() => setPageIndex((value) => Math.min(totalOfferPages - 1, value + 1))}
                disabled={pageIndex + 1 >= totalOfferPages || summaryLoading}
              >
                Próxima
              </button>
            </nav>
          )}
        </div>
      )}
      {tab === 'top' && (
        <div className="mobile-market-list" aria-live="polite">
          {topSales.map((sale, index) => (
            <article
              className="mobile-market-row mobile-top-sale"
              key={sale.currencyGroup + '-' + sale.itemName + '-' + index}
            >
              <span className="market-rank">{index + 1}</span>
              <span className="market-item-main">
                <MarketItemArtwork
                  assetPath={marketItemAssets[itemAssetKey({ name: sale.itemName })]}
                />
                <span className="market-item-copy">
                  <strong>{sale.itemName}</strong>
                  <small>
                    {numberFormat.format(sale.transactions)} vendas ·{' '}
                    {numberFormat.format(sale.quantity)} un.
                  </small>
                </span>
              </span>
              <span className="market-prices">
                {filter === 'all' ? (
                  <>
                    {sale.averageGoldUnitPrice !== null && (
                      <span>
                        <CurrencyIcon kind="gold" />
                        {numberFormat.format(sale.averageGoldUnitPrice)}
                      </span>
                    )}
                    {sale.averageOrbUnitPrice !== null && (
                      <span>
                        <CurrencyIcon kind="orbs" />
                        {numberFormat.format(sale.averageOrbUnitPrice)}
                      </span>
                    )}
                  </>
                ) : (
                  sale.averageUnitPrice !== null && (
                    <span>
                      {filter === 'gold' ? (
                        <CurrencyIcon kind="gold" />
                      ) : (
                        <CurrencyIcon kind="orbs" />
                      )}
                      {numberFormat.format(sale.averageUnitPrice)}
                    </span>
                  )
                )}
              </span>
            </article>
          ))}
          {topSales.length === 0 && (
            <p className="mobile-empty">Ainda não há vendas suficientes para este filtro.</p>
          )}
        </div>
      )}
      {tab === 'recent' && (
        <div className="mobile-market-list" aria-live="polite">
          {recentSales.map((sale) => (
            <article className="mobile-market-row mobile-recent-sale" key={sale.id}>
              {sale.kind === 'pokemon' ? (
                <PokemonSpriteView
                  pokemon={sale}
                  className="market-kind-icon kind-pokemon"
                  size={32}
                  disabled={mockMode}
                  lazy
                  fallback={<UiIcon name="pokemon" />}
                />
              ) : (
                <MarketItemArtwork
                  assetPath={marketItemAssets[itemAssetKey({ name: sale.name })]}
                  className="market-kind-icon"
                />
              )}
              <span className="market-item-main">
                {sale.kind === 'pokemon' ? null : (
                  <span className="market-item-copy market-recent-copy">
                    <strong>
                      {sale.quantity > 1 ? numberFormat.format(sale.quantity) + '× ' : ''}
                      {sale.name}
                    </strong>
                    <small>Item · {dateFormat.format(sale.occurredAt)}</small>
                  </span>
                )}
                {sale.kind === 'pokemon' && (
                  <span className="market-item-copy market-recent-copy">
                    <strong>
                      {sale.quantity > 1 ? numberFormat.format(sale.quantity) + '× ' : ''}
                      {sale.name}
                    </strong>
                    <small>Pokémon · {dateFormat.format(sale.occurredAt)}</small>
                  </span>
                )}
              </span>
              <span className="market-prices">
                {sale.unitPrice !== null && sale.quantity > 1 && (
                  <small>Un. {numberFormat.format(sale.unitPrice)}</small>
                )}
                <strong>
                  {numberFormat.format(sale.total)}
                  {sale.currency === 'gold' ? (
                    <CurrencyIcon kind="gold" />
                  ) : sale.currency === 'orb' ? (
                    <CurrencyIcon kind="orbs" />
                  ) : (
                    <span className="unknown-currency">?</span>
                  )}
                </strong>
              </span>
            </article>
          ))}
          {recentSales.length === 0 && (
            <p className="mobile-empty">
              Ainda não há vendas no histórico disponível para este filtro.
            </p>
          )}
        </div>
      )}
      <p className="mobile-footnote">
        O mobile não compra, edita regras nem ativa o Sniper. Os indicadores refletem apenas o
        Manager.
      </p>
    </section>
  )
}

function MarketItemArtwork({
  assetPath,
  className = 'market-item-artwork',
}: {
  assetPath?: string
  className?: string
}) {
  return (
    <span className={className} aria-hidden="true">
      {assetPath ? <img src={itemAssetUrl(assetPath)} alt="" /> : <UiIcon name="market" />}
    </span>
  )
}

function OfferRow({
  item,
  filter,
  assetPath,
}: {
  item: MobileMarketSummary
  filter: MarketFilter
  assetPath?: string
}) {
  return (
    <article className="mobile-market-row mobile-offer-row">
      <MarketItemArtwork assetPath={assetPath} />
      <span className="market-item-main">
        <span className="market-item-copy">
          <strong>{item.name}</strong>
          <small>
            {numberFormat.format(item.listings)} ofertas · {numberFormat.format(item.units)} un.
          </small>
        </span>
      </span>
      <span className="market-offer-count">{numberFormat.format(item.listings)}</span>
      <span className="market-unit-count">{numberFormat.format(item.units)}</span>
      <span className="market-prices">
        {item.minGold !== null && filter !== 'gems' && (
          <span>
            <CurrencyIcon kind="gold" />
            {numberFormat.format(item.minGold)}
          </span>
        )}
        {item.minOrb !== null && filter !== 'gold' && (
          <span>
            <CurrencyIcon kind="orbs" />
            {numberFormat.format(item.minOrb)}
          </span>
        )}
      </span>
    </article>
  )
}

function humanizeCategory(value: string) {
  const labels: Record<string, string> = {
    ball: 'Pokébolas',
    balls: 'Pokébolas',
    potion: 'Poções',
    potions: 'Poções',
    stone: 'Pedras',
    stones: 'Pedras',
    diamond: 'Diamantes',
    diamonds: 'Diamantes',
    fragment: 'Fragmentos',
    fragments: 'Fragmentos',
    bicycle: 'Bicicletas',
    bicycles: 'Bicicletas',
    box: 'Caixas',
    boxes: 'Caixas',
    tm: 'TMs',
    tms: 'TMs',
    house: 'Casas',
    houses: 'Casas',
    'mega stone': 'Mega Stones',
    'mega-stone': 'Mega Stones',
    'boss token': 'Boss Tokens',
    'boss-token': 'Boss Tokens',
    other: 'Outros',
  }
  if (labels[value.toLocaleLowerCase()]) return labels[value.toLocaleLowerCase()]!
  return value
    .replaceAll('_', ' ')
    .replaceAll('-', ' ')
    .replace(/\b\p{L}/gu, (letter) => letter.toLocaleUpperCase())
}

function humanizeStatus(value: string) {
  const normalized = value.toLocaleLowerCase()
  if (normalized.includes('read') || normalized.includes('ready') || normalized.includes('updated'))
    return 'Leitura atualizada'
  if (normalized.includes('wait') || normalized.includes('pending')) return 'Aguardando leitura'
  if (normalized.includes('error') || normalized.includes('fail')) return 'Leitura indisponível'
  return value ? value.replaceAll('_', ' ') : 'Aguardando leitura'
}

function SettingsPage({
  connection,
  snapshot,
  mockMode,
}: {
  connection: MobileConnection
  snapshot: MobileSnapshot | null
  mockMode: boolean
}) {
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'unavailable'>('idle')
  const address = window.location.origin + window.location.pathname
  const copyAddress = async () => {
    try {
      await navigator.clipboard.writeText(address)
      setCopyState('copied')
    } catch {
      setCopyState('unavailable')
    }
  }
  return (
    <section className="mobile-page" aria-labelledby="settings-title">
      <PageHeading
        id="settings-title"
        eyebrow="ESTADO DO COMPANION"
        title="Configurações"
        subtitle="Informações de acesso e diagnóstico"
      />
      <article className="mobile-panel settings-panel">
        <div className="settings-row">
          <span>Conexão com Manager</span>
          <strong className={'settings-value connection-label-' + connection}>
            {connection === 'connected'
              ? 'Conectado'
              : connection === 'loading'
                ? 'Conectando…'
                : 'Desconectado'}
          </strong>
        </div>
        <div className="settings-row">
          <span>Modo</span>
          <strong className="settings-value">
            {mockMode ? 'Demonstração' : 'Somente leitura'}
          </strong>
        </div>
        <div className="settings-row">
          <span>Contas visíveis</span>
          <strong className="settings-value">{snapshot?.accounts.length ?? 0}</strong>
        </div>
        <div className="settings-row">
          <span>Endereço deste dispositivo</span>
          <code className="settings-address">{address}</code>
        </div>
        <button
          className="mobile-secondary-button"
          type="button"
          onClick={() => void copyAddress()}
        >
          {copyState === 'copied' ? 'Endereço copiado' : 'Copiar endereço'}
        </button>
        {copyState === 'unavailable' && (
          <small className="copy-fallback">
            Seu navegador não liberou a área de transferência; selecione e copie o endereço acima.
          </small>
        )}
      </article>
      <article className="mobile-panel settings-note">
        <span className="settings-safe-mark">
          <MobileGlyph kind="safe" />
        </span>
        <div>
          <strong>Área protegida</strong>
          <p>
            Esta prévia não exibe nem altera credenciais, compras ou configurações críticas.
          </p>
        </div>
      </article>
    </section>
  )
}

const pageInfo: Record<MobilePageName, { label: string; icon: UiIconName }> = {
  dashboard: { label: 'Dashboard', icon: 'dashboard' },
  automations: { label: 'Automações', icon: 'automation' },
  inventory: { label: 'Inventário', icon: 'inventory' },
  market: { label: 'Mercado', icon: 'market' },
  settings: { label: 'Mais', icon: 'settings' },
}

export default function MobileApp() {
  const now = useSharedClock()
  const mockMode = new URLSearchParams(window.location.search).get('mode') === 'mock'
  const { snapshot, connection, snapshotAt } = useMobileSnapshot(mockMode)
  const [page, setPage] = useState<MobilePageName>('dashboard')
  const accounts = snapshot?.accounts ?? []

  let content: ReactNode
  switch (page) {
    case 'dashboard':
      content = (
        <DashboardPage
          snapshot={snapshot}
          accounts={accounts}
          now={now}
          snapshotAt={snapshotAt}
          mockMode={mockMode}
          connection={connection}
        />
      )
      break
    case 'automations':
      content = <AutomationPage accounts={accounts} now={now} snapshotAt={snapshotAt} />
      break
    case 'inventory':
      content = <InventoryPage accounts={accounts} mockMode={mockMode} />
      break
    case 'market':
      content = <MarketPage snapshot={snapshot} mockMode={mockMode} />
      break
    case 'settings':
      content = <SettingsPage connection={connection} snapshot={snapshot} mockMode={mockMode} />
      break
  }

  return (
    <main className="mobile-shell">
      <header className="mobile-topbar">
        <a
          className="mobile-brand"
          href="#inicio"
          onClick={(event) => {
            event.preventDefault()
            setPage('dashboard')
          }}
          aria-label="Pokeidle Manager, início"
        >
          <span
            aria-hidden="true"
            style={{
              width: 36,
              height: 36,
              display: 'grid',
              placeItems: 'center',
              border: '1px solid #d49a56',
              borderRadius: '10px 4px',
              color: '#ffe5a2',
              background: 'linear-gradient(145deg, #58402d, #271d27 65%)',
              boxShadow: 'inset 0 0 0 3px #d49a561c',
              fontWeight: 900,
            }}
          >P</span>
          <span>POKEIDLE<small>MANAGER</small></span>
        </a>
        <div className="mobile-topbar-right">
          <span className={'mobile-preview-badge' + (mockMode ? ' mock-badge' : '')}>
            {mockMode ? 'MOCK' : 'LIVE'}
          </span>
          <span
            className={'mobile-mini-connection connection-' + connection}
            aria-label={
              connection === 'connected'
                ? 'Conectado'
                : connection === 'loading'
                  ? 'Conectando'
                  : 'Desconectado'
            }
          />
        </div>
      </header>
      {content}
      <nav className="mobile-bottom-nav" aria-label="Navegação principal">
        {(Object.keys(pageInfo) as MobilePageName[]).map((name) => (
          <button
            key={name}
            className={'mobile-nav-item' + (page === name ? ' is-current' : '')}
            type="button"
            aria-current={page === name ? 'page' : undefined}
            onClick={() => setPage(name)}
          >
            <span className="mobile-nav-icon" aria-hidden="true">
              <UiIcon name={pageInfo[name].icon} />
            </span>
            <span>{pageInfo[name].label}</span>
          </button>
        ))}
      </nav>
    </main>
  )
}
