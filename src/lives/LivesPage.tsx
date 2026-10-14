import { useMemo, useState } from 'react'
import headerScenery from '../assets/pokeidle-theme/backgrounds/shared-header-scenery.png'
import titleboard from '../assets/pokeidle-theme/backgrounds/shared-header-titleboard.png'
import twitchIcon from '../assets/pokeidle-theme/backgrounds/icon-twitch.png'
import { kickChannelUrl, twitchChannelUrl } from './urls'
import type { LiveSnapshot } from './types'
import './LivesPage.css'

export type LivesPageProps = {
  snapshot: LiveSnapshot
  mockMode?: boolean
  onOpenLive: (platform: 'twitch' | 'kick', identifier: string) => Promise<void>
}

type LiveCardItem = {
  platform: 'twitch' | 'kick'
  identifier: string
  name: string
  viewers: number
  url: string | null
}

const numberFormat = new Intl.NumberFormat('pt-BR')
const dateTimeFormat = new Intl.DateTimeFormat('pt-BR', {
  dateStyle: 'short',
  timeStyle: 'short',
})

function sortLives(items: LiveCardItem[]) {
  return items.sort(
    (left, right) => right.viewers - left.viewers || left.name.localeCompare(right.name, 'pt-BR'),
  )
}

function latestByIdentifier<T extends { login: string }>(items: T[]): T[] {
  const latest = new Map<string, T>()
  for (const item of items) latest.set(item.login.toLocaleLowerCase('en-US'), item)
  return [...latest.values()]
}

function latestKickByIdentifier<T extends { slug: string }>(items: T[]): T[] {
  const latest = new Map<string, T>()
  for (const item of items) latest.set(item.slug.toLocaleLowerCase('en-US'), item)
  return [...latest.values()]
}

function formatUpdatedAt(value: number | null) {
  if (value === null || !Number.isFinite(value)) return null
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : dateTimeFormat.format(date)
}

function PlatformMark({ platform }: { platform: 'twitch' | 'kick' }) {
  return (
    <span className={`lives-platform-mark lives-platform-mark-${platform}`} aria-hidden="true">
      {platform === 'twitch' ? <img src={twitchIcon} alt="" /> : 'K'}
    </span>
  )
}

function LiveCard({
  item,
  onOpenLive,
}: {
  item: LiveCardItem
  onOpenLive: LivesPageProps['onOpenLive']
}) {
  const [openFailed, setOpenFailed] = useState(false)
  const label = item.platform === 'twitch' ? 'Twitch' : 'Kick'

  const open = async () => {
    if (!item.url) return
    setOpenFailed(false)
    try {
      await onOpenLive(item.platform, item.identifier)
    } catch {
      setOpenFailed(true)
    }
  }

  return (
    <article className={`lives-stream-card lives-stream-card-${item.platform}`}>
      <div className="lives-stream-identity">
        <PlatformMark platform={item.platform} />
        <div className="lives-stream-copy">
          <h3>{item.name}</h3>
          <span className="lives-stream-handle">
            {item.platform === 'twitch' ? '@' : ''}
            {item.identifier}
          </span>
        </div>
      </div>
      <div className="lives-stream-meta">
        <span className="lives-viewers">
          <span aria-hidden="true">◉</span> {numberFormat.format(item.viewers)}{' '}
          <small>espectadores</small>
        </span>
        <span className="lives-online-indicator">
          <i aria-hidden="true" /> Ao vivo
        </span>
      </div>
      {item.url ? (
        <button
          className="lives-open-button"
          type="button"
          onClick={() => void open()}
          aria-label={`Abrir ${item.name} na ${label}`}
        >
          Abrir canal <span aria-hidden="true">↗</span>
        </button>
      ) : (
        <span className="lives-unavailable" role="status">
          Link indisponível
        </span>
      )}
      {openFailed && (
        <span className="lives-open-error" role="status">
          Não foi possível abrir o canal.
        </span>
      )}
    </article>
  )
}

export function LivesPage({ snapshot, mockMode = false, onOpenLive }: LivesPageProps) {
  const twitchLives = useMemo(
    () =>
      sortLives(
        latestByIdentifier(snapshot.twitch).map((live) => ({
          platform: 'twitch' as const,
          identifier: live.login,
          name: live.name,
          viewers: Number.isFinite(live.viewers) && live.viewers >= 0 ? live.viewers : 0,
          url: twitchChannelUrl(live.login),
        })),
      ),
    [snapshot.twitch],
  )

  const kickLives = useMemo(
    () =>
      sortLives(
        latestKickByIdentifier(snapshot.kick)
          .filter((live) => live.online)
          .map((live) => ({
            platform: 'kick' as const,
            identifier: live.slug,
            name: live.name,
            viewers: Number.isFinite(live.viewers) && live.viewers >= 0 ? live.viewers : 0,
            url: kickChannelUrl(live.slug),
          })),
      ),
    [snapshot.kick],
  )

  const updatedAt = formatUpdatedAt(snapshot.updatedAt)
  const hasAnyLive = twitchLives.length > 0 || kickLives.length > 0
  const hasLiveSnapshot = snapshot.updatedAt !== null

  return (
    <main className="page lives-page">
      <header className="shared-page-header">
        <img className="shared-page-header-scenery" src={headerScenery} alt="" aria-hidden="true" />
        <div className="shared-page-header-plaque">
          <img src={titleboard} alt="" aria-hidden="true" />
          <div className="shared-page-header-copy">
            <h1>Lives</h1>
          </div>
        </div>
        <div className="lives-header-caption">
          <span className="lives-header-spark" aria-hidden="true">
            ✦
          </span>
          <span>Acompanhe quem está ao vivo</span>
        </div>
      </header>

      <div className="lives-page-body">
        <section className="lives-source-strip" aria-label="Origem dos dados">
          <div className="lives-source-copy">
            <span
              className={`lives-source-dot${snapshot.stale ? ' is-stale' : ''}`}
              aria-hidden="true"
            />
            <span>
              {snapshot.sourceAccountNick ? (
                <>
                  Leitura por <strong>{snapshot.sourceAccountNick}</strong>
                </>
              ) : (
                'Aguardando uma conta leitora'
              )}
            </span>
            {snapshot.sourceTransport && (
              <span className="lives-source-transport">
                {snapshot.sourceTransport === 'browser'
                  ? 'Navegador'
                  : snapshot.sourceTransport === 'background'
                    ? 'Background'
                    : snapshot.sourceTransport === 'transition'
                      ? 'Em transição'
                      : 'Desconectada'}
              </span>
            )}
            {mockMode && <span className="lives-mock-badge">Prévia</span>}
          </div>
          <div className="lives-update-copy" aria-live="polite">
            {snapshot.stale && <span className="lives-stale-label">Dados desatualizados</span>}
            {updatedAt && (
              <span>
                {snapshot.stale ? 'Última atualização' : 'Atualizado'} às {updatedAt}
              </span>
            )}
          </div>
        </section>

        {!snapshot.sourceAccountId && (
          <div className="lives-notice lives-notice-empty" role="status">
            <strong>Nenhuma conta está lendo as lives.</strong>
            <span>Conecte uma conta ao jogo para começar a acompanhar as transmissões.</span>
          </div>
        )}

        {snapshot.sourceAccountId && !hasLiveSnapshot && (
          <div className="lives-notice lives-notice-empty" role="status">
            <strong>Aguardando os primeiros dados de lives.</strong>
            <span>A lista será preenchida quando a conta receber o estado do jogo.</span>
          </div>
        )}

        {snapshot.stale && snapshot.sourceAccountId && (
          <div className="lives-notice lives-notice-stale" role="status">
            A conexão pode ter sido interrompida. Os últimos dados recebidos continuam visíveis.
          </div>
        )}

        <div className="lives-platform-grid">
          <section className="lives-platform-section" aria-labelledby="lives-twitch-title">
            <div className="lives-section-heading">
              <div className="lives-section-title-wrap">
                <PlatformMark platform="twitch" />
                <div>
                  <p className="lives-eyebrow">TRANSMISSÕES ATIVAS</p>
                  <h2 id="lives-twitch-title">Twitch</h2>
                </div>
              </div>
              <span className="lives-count">{twitchLives.length}</span>
            </div>
            {twitchLives.length ? (
              <div className="lives-stream-list">
                {twitchLives.map((item) => (
                  <LiveCard
                    key={`twitch:${item.identifier.toLowerCase()}`}
                    item={item}
                    onOpenLive={onOpenLive}
                  />
                ))}
              </div>
            ) : hasLiveSnapshot ? (
              <div className="lives-platform-empty">
                <span aria-hidden="true">◌</span>
                <p>Nenhuma live da Twitch está online no momento.</p>
              </div>
            ) : (
              <div className="lives-platform-empty">
                <p>{snapshot.sourceAccountId ? 'Aguardando a lista da Twitch.' : 'Conecte uma conta para carregar as lives.'}</p>
              </div>
            )}
          </section>

          <section className="lives-platform-section" aria-labelledby="lives-kick-title">
            <div className="lives-section-heading">
              <div className="lives-section-title-wrap">
                <PlatformMark platform="kick" />
                <div>
                  <p className="lives-eyebrow">TRANSMISSÕES ATIVAS</p>
                  <h2 id="lives-kick-title">Kick</h2>
                </div>
              </div>
              <span className="lives-count">{kickLives.length}</span>
            </div>
            {kickLives.length ? (
              <div className="lives-stream-list">
                {kickLives.map((item) => (
                  <LiveCard
                    key={`kick:${item.identifier.toLowerCase()}`}
                    item={item}
                    onOpenLive={onOpenLive}
                  />
                ))}
              </div>
            ) : hasLiveSnapshot ? (
              <div className="lives-platform-empty">
                <span aria-hidden="true">◌</span>
                <p>Nenhuma live da Kick está online no momento.</p>
              </div>
            ) : (
              <div className="lives-platform-empty">
                <p>{snapshot.sourceAccountId ? 'Aguardando a lista da Kick.' : 'Conecte uma conta para carregar as lives.'}</p>
              </div>
            )}
          </section>
        </div>

        {snapshot.sourceAccountId && hasLiveSnapshot && !hasAnyLive && (
          <p className="lives-no-streams-note">
            Quando uma transmissão entrar ao vivo, ela aparecerá aqui.
          </p>
        )}
      </div>
    </main>
  )
}
