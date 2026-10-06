import { invoke } from '@tauri-apps/api/core'
import { useEffect, useMemo, useState } from 'react'
import { Badge, Button, Card } from '../components/primitives'
import headerScenery from '../assets/pokeidle-theme/backgrounds/shared-header-scenery.png'
import titleboard from '../assets/pokeidle-theme/backgrounds/shared-header-titleboard.png'
import hpIcon from '../assets/pokeidle-theme/backgrounds/where-hunt-hp.png'
import attackIcon from '../assets/pokeidle-theme/backgrounds/where-hunt-attack.png'
import defenseIcon from '../assets/pokeidle-theme/backgrounds/where-hunt-defense.png'
import specialAttackIcon from '../assets/pokeidle-theme/backgrounds/where-hunt-special-attack.png'
import speedIcon from '../assets/pokeidle-theme/backgrounds/where-hunt-speed.png'
import vipBonusIcon from '../assets/pokeidle-theme/backgrounds/where-hunt-vip.png'
import guildBonusIcon from '../assets/pokeidle-theme/backgrounds/where-hunt-guild.png'
import guildBoostIcon from '../assets/pokeidle-theme/backgrounds/where-hunt-guild-boost.png'
import twitchBonusIcon from '../assets/pokeidle-theme/backgrounds/where-hunt-twitch.webp'
import { useGameItemCatalog } from '../inventory/assets'
import { PokemonAsset } from '../inventory/asset-components'
import { selectVisibleAccounts, useAppStore } from '../stores/app-store'
import type { AccountView, DepotPokemon } from '../types'
import { estimateHunts, hasCompleteCalculationData, readHuntReference, type HuntEstimate, type HuntReference } from './calculator'

type Ranking = 'trainer' | 'pokemon' | 'gold'
const activePokemonStats = [
  { key: 'hp', label: 'HP', icon: hpIcon },
  { key: 'atk', label: 'ATK', icon: attackIcon },
  { key: 'def', label: 'DEF', icon: defenseIcon },
  { key: 'spAtk', label: 'SP. ATK', icon: specialAttackIcon },
  { key: 'spDef', label: 'SP. DEF', icon: defenseIcon },
  { key: 'speed', label: 'SPD', icon: speedIcon },
] as const
type BonusKey = 'vip' | 'guild' | 'guild-boost' | 'twitch' | 'event'
const bonusIcons: Partial<Record<BonusKey, string>> = {
  vip: vipBonusIcon,
  guild: guildBonusIcon,
  'guild-boost': guildBoostIcon,
  twitch: twitchBonusIcon,
}
const compact = new Intl.NumberFormat('pt-BR', { notation: 'compact', maximumFractionDigits: 1 })
const full = new Intl.NumberFormat('pt-BR')
const percent = new Intl.NumberFormat('pt-BR', { maximumFractionDigits: 1 })
let huntReferenceRequest: Promise<HuntReference> | null = null

function requestHuntReference() {
  if (!huntReferenceRequest) {
    const request = invoke<unknown>('where_to_hunt_reference').then((value) => {
      const parsed = readHuntReference(value)
      if (!parsed) throw new Error('A referência recebida está incompleta.')
      useAppStore.getState().setWhereHuntReference(parsed)
      return parsed
    })
    huntReferenceRequest = request.finally(() => {
      huntReferenceRequest = null
    })
  }
  return huntReferenceRequest
}

type BonusRow = { key: BonusKey; label: string; percentage: number; active: boolean }

function activeBonusRows(account: AccountView, kind: Ranking): BonusRow[] {
  const bonus = account.xpBonus
  const eventPct = kind === 'pokemon' ? bonus?.eventPokemonPct : bonus?.eventTrainerPct
  const guildRankPct = bonus?.guildRankPct ?? 0
  const twitchPct = bonus?.twitchPct ?? 0

  return [
    { key: 'vip', label: 'VIP', percentage: 50, active: Boolean(account.vip) },
    { key: 'guild', label: 'Guilda', percentage: guildRankPct, active: guildRankPct > 0 },
    { key: 'guild-boost', label: 'Boost da guilda', percentage: 10, active: Boolean(bonus?.guildBoostActive) },
    { key: 'twitch', label: 'Twitch', percentage: twitchPct, active: twitchPct > 0 },
    { key: 'event', label: 'Evento', percentage: eventPct ?? 0, active: (eventPct ?? 0) > 0 },
  ]
}

function activePokemonView(account: AccountView): DepotPokemon | null {
  const active = account.activePokemon
  if (!active) return null
  return {
    id: active.id,
    name: active.name,
    level: active.level,
    speciesId: active.speciesId,
    looktype: active.looktype,
    lookShiny: active.lookShiny,
    shiny: active.shiny,
    quality: active.quality,
    power: active.potency,
    types: active.types,
    locked: false,
  }
}

export function WhereToHunt() {
  const accounts = useAppStore(selectVisibleAccounts)
  const selectedAccountId = useAppStore((state) => state.ui.whereToHuntAccountId)
  const setSelectedAccountId = useAppStore((state) => state.setWhereToHuntAccountId)
  const reference = useAppStore((state) => state.whereHuntReference)
  const catalog = useGameItemCatalog()
  const [loading, setLoading] = useState(() => !reference)
  const [loadError, setLoadError] = useState('')
  const [ranking, setRanking] = useState<Ranking>('trainer')
  const [query, setQuery] = useState('')
  const [region, setRegion] = useState('all')
  const account = accounts.find((entry) => entry.id === selectedAccountId) ?? accounts[0]

  useEffect(() => {
    let current = true
    if (reference) {
      setLoading(false)
      setLoadError('')
      return () => { current = false }
    }
    if (!('__TAURI_INTERNALS__' in window)) {
      setLoading(false)
      setLoadError('A referência de hunts fica disponível ao abrir o programa desktop.')
      return () => { current = false }
    }
    setLoading(true)
    void requestHuntReference()
      .then(() => {
        if (current) setLoadError('')
      })
      .catch((error: unknown) => {
        if (current) setLoadError(error instanceof Error ? error.message : 'Não foi possível carregar os dados das hunts.')
      })
      .finally(() => { if (current) setLoading(false) })
    return () => { current = false }
  }, [reference])

  const estimates = useMemo(() => {
    if (!account || !reference) return []
    const needle = query.trim().toLocaleLowerCase('pt-BR')
    return estimateHunts(account, reference)
      .filter((result) => {
        const canonicalRegion = reference.hunts.find((hunt) => hunt.s === result.hunt.s)?.a
        return (!needle || `${result.hunt.n} ${result.hunt.a}`.toLocaleLowerCase('pt-BR').includes(needle))
          && (region === 'all' || canonicalRegion === region)
      })
      .sort((left, right) => ranking === 'gold'
        ? right.goldPerHour - left.goldPerHour
        : ranking === 'pokemon'
          ? right.xpPokemonPerHour - left.xpPokemonPerHour
          : right.xpTrainerPerHour - left.xpTrainerPerHour)
  }, [account, query, ranking, reference, region])
  const regions = useMemo(() => [...new Set((reference?.hunts ?? []).map((hunt) => hunt.a))].sort(), [reference])

  const activePokemon = account ? activePokemonView(account) : null
  const potency = Number(account?.activePokemon?.potency)
  const potencyBadgeClass = activePokemon?.shiny
    ? 'where-hunt-potency-shiny'
    : Number.isInteger(potency) && potency >= 1 && potency <= 5
      ? `where-hunt-potency-p${potency}`
      : 'where-hunt-potency-unknown'
  const missingAccountData = Boolean(account && !hasCompleteCalculationData(account))
  const xpBonuses = account ? activeBonusRows(account, ranking) : []
  const xpRate = (estimate: HuntEstimate) => ranking === 'pokemon' ? estimate.xpPokemonPerHour : estimate.xpTrainerPerHour

  return (
    <main className="page where-hunt-page">
      <header className="shared-page-header">
        <img className="shared-page-header-scenery" src={headerScenery} alt="" aria-hidden="true" />
        <div className="shared-page-header-plaque">
          <img src={titleboard} alt="" aria-hidden="true" />
          <div className="shared-page-header-copy"><h1>Onde Caçar</h1></div>
        </div>
        <div className="shared-page-header-actions where-hunt-account">
          <label htmlFor="where-hunt-account">Conta para simular</label>
          <select id="where-hunt-account" value={account?.id ?? ''} onChange={(event) => setSelectedAccountId(event.target.value)}>
            {accounts.length === 0 && <option value="">Nenhuma conta</option>}
            {accounts.map((item) => <option key={item.id} value={item.id}>{item.nick}</option>)}
          </select>
        </div>
      </header>

      {!account ? (
        <Card className="where-hunt-empty"><h2>Conecte uma conta primeiro</h2><p>Quando uma conta estiver conectada, vou usar o Pokémon ativo dela para estimar as melhores hunts.</p></Card>
      ) : (
        <>
          <section className="where-hunt-overview">
            <Card className="where-hunt-active">
              <div className="where-hunt-card-label">POKÉMON ATIVO</div>
              {activePokemon ? (
                <div className="where-hunt-active-layout">
                  <div className="where-hunt-active-art">
                    <PokemonAsset pokemon={activePokemon} catalog={catalog} size={126} />
                  </div>
                  <div className="where-hunt-active-heading">
                    <div className="where-hunt-active-copy">
                      <h2>{activePokemon.name}</h2>
                      <p>Nv. {full.format(activePokemon.level)} <i aria-hidden="true">·</i> Treinador Nv. {full.format(account.trainerLevel ?? 0)}</p>
                    </div>
                    <div className="where-hunt-quality">
                      <small>QUALIDADE</small>
                      <strong>{activePokemon.quality?.toFixed(3) ?? '—'}</strong>
                      <span className={`where-hunt-potency ${potencyBadgeClass}`}>
                        <Badge tone={activePokemon.shiny ? 'warning' : 'neutral'}>
                          {activePokemon.shiny ? 'Shiny ×3' : `P${account.activePokemon?.potency ?? '—'}`}
                        </Badge>
                      </span>
                    </div>
                  </div>
                  <div
                    className="where-hunt-iv-grid"
                    role="group"
                    aria-label="IVs individuais do Pokémon ativo"
                  >
                    {activePokemonStats.map(({ key, label, icon }) => (
                      <div className={`where-hunt-iv-card where-hunt-iv-card-${key}`} key={key}>
                        <span className={`where-hunt-iv-icon where-hunt-iv-icon-${key}`} aria-hidden="true"><img src={icon} alt="" /></span>
                        <small>{label}</small>
                        <strong>{account.activePokemon?.ivs?.[key] ?? '—'}</strong>
                      </div>
                    ))}
                  </div>
                </div>
              ) : <p>Não foi possível identificar o Pokémon ativo nesta conta.</p>}
            </Card>
            <Card className="where-hunt-bonus-card">
              <h2 className="where-hunt-bonus-title"><span aria-hidden="true">✦</span>BÔNUS CONSIDERADOS<span aria-hidden="true">✦</span></h2>
              <div className="where-hunt-bonus-list" role="list" aria-label="Bônus de XP">
                {xpBonuses.map((bonus) => (
                  <div className="where-hunt-bonus-row" data-bonus={bonus.key} data-active={bonus.active} role="listitem" key={bonus.key}>
                    <span className="where-hunt-bonus-icon" aria-hidden="true">{bonusIcons[bonus.key] ? <img src={bonusIcons[bonus.key]} alt="" /> : '✦'}</span>
                    <span className="where-hunt-bonus-name">{bonus.label}</span>
                    <strong className="where-hunt-bonus-value" aria-label={bonus.active ? `+${percent.format(bonus.percentage)}% ativo` : 'Desativado'}>
                      {bonus.active ? `+${percent.format(bonus.percentage)}%` : 'OFF'}
                    </strong>
                  </div>
                ))}
              </div>
              <small>{ranking === 'gold'
                ? 'Bônus de XP não alteram o ranking por ouro. Boosts individuais da loja ainda não estão disponíveis no snapshot.'
                : 'Os bônus detectados são multiplicados. Boosts individuais comprados na loja ainda podem não estar disponíveis no snapshot.'}</small>
            </Card>
          </section>

          <section className="where-hunt-results card">
            <div className="where-hunt-results-head">
              <div><p className="where-hunt-card-label">RECOMENDAÇÕES · {estimates.length} HUNTS</p><h2>Onde rende mais?</h2></div>
              <div className="where-hunt-ranking" role="group" aria-label="Ordenar hunts por">
                <Button className={ranking === 'trainer' ? 'active' : ''} onClick={() => setRanking('trainer')}>XP treinador</Button>
                <Button className={ranking === 'pokemon' ? 'active' : ''} onClick={() => setRanking('pokemon')}>XP Pokémon</Button>
                <Button className={ranking === 'gold' ? 'active' : ''} onClick={() => setRanking('gold')}>Ouro</Button>
              </div>
            </div>
            <div className="where-hunt-controls">
              <input aria-label="Buscar hunt ou região" placeholder="Buscar hunt ou região..." value={query} onChange={(event) => setQuery(event.target.value)} />
              <label className="where-hunt-region">Região<select aria-label="Filtrar região" value={region} onChange={(event) => setRegion(event.target.value)}><option value="all">Todas</option>{regions.map((item) => <option key={item} value={item}>{item}</option>)}</select></label>
            </div>
            {loadError && <p className="where-hunt-notice" role="status">{loadError}</p>}
            {missingAccountData && <p className="where-hunt-notice" role="status">Faltam IVs individuais do Pokémon ativo. Aguarde a próxima atualização do estado da conta para calcular.</p>}
            {!loading && !loadError && !estimates.length && <p className="where-hunt-notice">{missingAccountData ? 'O cálculo vai aparecer quando os dados do Pokémon estiverem completos.' : 'Nenhuma hunt compatível com o nível da conta foi encontrada.'}</p>}
            <div className="where-hunt-list">
              {estimates.slice(0, 80).map((result, index) => (
                <article className="where-hunt-result" key={result.hunt.s}>
                  <span className="where-hunt-rank">{String(index + 1).padStart(2, '0')}</span>
                  <div className="where-hunt-name">
                    {result.primarySpecies.looktype ? (
                      <span className="where-hunt-sprite" aria-hidden="true">
                        <PokemonAsset
                          pokemon={{
                            id: `hunt-${result.primarySpecies.id}`,
                            name: result.primarySpecies.n,
                            level: result.hunt.nv,
                            speciesId: result.primarySpecies.id,
                            looktype: result.primarySpecies.looktype,
                            types: result.primarySpecies.t,
                            locked: false,
                          }}
                          catalog={catalog}
                          size={44}
                        />
                      </span>
                    ) : null}
                    <div className="where-hunt-name-copy"><h3>{result.hunt.n}</h3><span>{result.hunt.a} · Nv. {full.format(result.hunt.nv)}</span></div>
                  </div>
                  <div className="where-hunt-stat where-hunt-xp"><small>{ranking === 'pokemon' ? 'XP POKÉMON / H' : 'XP TREINADOR / H'}</small><strong>{compact.format(xpRate(result))}</strong></div>
                  <div className="where-hunt-stat where-hunt-gold"><small>OURO / H</small><strong>{compact.format(result.goldPerHour)}</strong></div>
                  <div className="where-hunt-stat where-hunt-kills"><small>ABATES / H</small><strong>{full.format(result.killsPerHour)}</strong></div>
                  <div className="where-hunt-stat where-hunt-ttk"><small>TEMPO / ABATE</small><strong>{result.secondsPerKill.toLocaleString('pt-BR')}s</strong></div>
                  <span className="where-hunt-confidence">ESTIMATIVA</span>
                </article>
              ))}
            </div>
            <footer className="where-hunt-disclaimer">Projeção inicial baseada nas fórmulas e no catálogo público do guia (app.js v82), nos spawns disponíveis para esta conta e no Pokémon ativo. O tempo por abate usa DPS médio, não a fila completa de ataques do simulador. Ouro bruto inclui ouro de abate e drops esperados; não inclui capturas vendidas, bolas ou custos. Ainda não estima mortes, poções, TM em área, movimento nem acesso individual a cada mapa. Use os valores para comparar tendências, não como promessa de rendimento.</footer>
          </section>
        </>
      )}
    </main>
  )
}
