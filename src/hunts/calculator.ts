import type { AccountView } from '../types'

export type ReferenceSpecies = {
  id: number
  n: string
  looktype?: number
  t: string[]
  b: number[]
  m: [string, string, number, number, number, number][]
  hl?: number
  l?: [string, number, number, number, number][]
}

export type ReferenceHunt = {
  s: string
  n: string
  a: string
  nv: number
  e: [number, number][]
}

export type HuntReference = {
  tipos: Record<string, Record<string, number>>
  looktypes?: Record<string, number>
  especies: ReferenceSpecies[]
  hunts: ReferenceHunt[]
}

export type HuntEstimate = {
  hunt: ReferenceHunt
  primarySpecies: ReferenceSpecies
  xpTrainerPerHour: number
  xpPokemonPerHour: number
  goldPerHour: number
  killsPerHour: number
  secondsPerKill: number
  weakLink?: string
}

export function hasCompleteCalculationData(account: AccountView) {
  const active = account.activePokemon
  const ivs = active?.ivs
  return Boolean(active?.speciesId && active.quality && active.potency
    && ivs && [ivs.hp, ivs.atk, ivs.def, ivs.spAtk, ivs.spDef, ivs.speed].every(Number.isFinite))
}

const QUALITY_EXPONENT = [0.95, 0.8, 0.8, 0.8, 0.8, 0.95]
const POTENCY_BONUS = [0, 0.05, 0.1, 0.25, 1]

const speciesById = (reference: HuntReference) => new Map(reference.especies.map((species) => [species.id, species]))

const gameXpPerKill = (level: number) => level <= 150
  ? Math.floor(0.6 * level * level) + 8
  : Math.floor(13_500 * (level / 150) ** 1.25)

const gameGoldPerKill = (level: number) => Math.round(36 * Math.min(level, 25_000) ** 0.6007)

const adjustedEffectiveness = (value: number) => value > 1 ? 1 + (value - 1) * 1.5 : value < 1 ? value / 1.5 : 1

function activeStats(account: AccountView, activeSpecies: ReferenceSpecies) {
  const active = account.activePokemon!
  const ivs = active.ivs
  if (!ivs || [ivs.hp, ivs.atk, ivs.def, ivs.spAtk, ivs.spDef, ivs.speed].some((value) => value === undefined)) return null
  const quality = active.quality
  const potency = active.potency
  if (!quality || !potency || potency < 1 || potency > 5) return null
  const factor = (1 + POTENCY_BONUS[potency - 1]) * (active.shiny ? 3 : 1)
  return activeSpecies.b.slice(0, 6).map((base, index) => {
    const iv = [ivs.hp, ivs.atk, ivs.def, ivs.spAtk, ivs.spDef, ivs.speed][index] ?? 16
    return Math.round((base + 2 * iv) * active.level / 100 * quality ** QUALITY_EXPONENT[index] * factor)
  })
}

function attackDamage(
  level: number,
  power: number,
  attack: number,
  defense: number,
  stab: boolean,
  effectiveness: number,
) {
  return ((((2 * level / 5 + 2) * power * Math.max(1, attack) / Math.max(1, defense)) / 50) + 2)
    * (stab ? 1.5 : 1) * adjustedEffectiveness(effectiveness) * 0.925
}

function dpsAgainst(
  account: AccountView,
  activeSpecies: ReferenceSpecies,
  activeStatsValue: number[],
  wild: ReferenceSpecies,
  wildLevel: number,
  typeChart: HuntReference['tipos'],
) {
  const level = account.activePokemon!.level
  const ivSpeed = account.activePokemon!.ivs?.speed ?? 16
  const moves = activeSpecies.m.filter((move) => move[5] <= level)
  let dps = 0
  let attacksPerSecond = 0
  const tackleMultiplier = wild.t.reduce((total, type) => total * (typeChart.NORMAL?.[type] ?? 1), 1)
  const tackleDamage = attackDamage(level, 30, activeStatsValue[1], (wild.b[2] + 32) * wildLevel / 100,
    activeSpecies.t.includes('NORMAL'), tackleMultiplier)
  for (const move of moves) {
    const [, moveType, special, power, cooldownMs] = move
    const attack = activeStatsValue[special ? 3 : 1]
    const defense = (special ? wild.b[4] : wild.b[2]) + 32
    const scaledDefense = defense * wildLevel / 100
    const multiplier = wild.t.reduce((total, type) => total * (typeChart[moveType]?.[type] ?? 1), 1)
    const cooldown = Math.max(400, cooldownMs - Math.min(32, Math.max(1, Math.round(ivSpeed))) * 10) / 1000
    const stab = activeSpecies.t.includes(moveType)
    dps += attackDamage(level, power, attack, scaledDefense, stab, multiplier) / cooldown
    attacksPerSecond += 1 / cooldown
  }
  const tackleRate = Math.max(0, 1 - attacksPerSecond * 0.9) / 2
  return dps + tackleDamage * tackleRate
}

export function bonusMultiplier(account: AccountView, trainer: boolean) {
  const xp = account.xpBonus
  return (account.vip ? 1.5 : 1)
    * (1 + (xp?.guildRankPct ?? 0) / 100)
    * (xp?.guildBoostActive ? 1.1 : 1)
    * (1 + (xp?.twitchPct ?? 0) / 100)
    * (1 + ((trainer ? xp?.eventTrainerPct : xp?.eventPokemonPct) ?? 0) / 100)
}

export function estimateHunts(account: AccountView, reference: HuntReference): HuntEstimate[] {
  const active = account.activePokemon
  if (!active?.speciesId || !active.ivs || !active.quality || !active.potency) return []
  const species = speciesById(reference)
  const activeSpecies = species.get(active.speciesId)
  const stats = activeSpecies && activeStats(account, activeSpecies)
  if (!activeSpecies || !stats) return []
  const rankXpTrainer = bonusMultiplier(account, true)
  const rankXpPokemon = bonusMultiplier(account, false)
  const accountHunts = new Map((account.huntOptions ?? []).map((hunt) => [hunt.slug, hunt]))
  const accountCatalogHunts = reference.hunts.filter((hunt) => accountHunts.has(hunt.s))
  const availableHunts = accountCatalogHunts.length ? accountCatalogHunts : reference.hunts

  return availableHunts
    .filter((hunt) => (accountHunts.get(hunt.s)?.level ?? hunt.nv) <= (account.trainerLevel ?? 0))
    .map((hunt) => {
      const observedHunt = accountHunts.get(hunt.s)
      const remoteComposition = observedHunt?.species
        ?.filter((entry): entry is { speciesId: number; weight: number } =>
          typeof entry.speciesId === 'number' && typeof entry.weight === 'number',
        )
        .map((entry) => [entry.speciesId, entry.weight] as [number, number])
      let composition = remoteComposition?.length ? remoteComposition : hunt.e
      if (composition.length && !composition.some(([id]) => species.has(id))) composition = hunt.e
      if (!composition.length) return null
      const effectiveHunt = {
        ...hunt,
        n: observedHunt?.name ?? hunt.n,
        a: observedHunt?.region ?? observedHunt?.area ?? hunt.a,
        nv: observedHunt?.level ?? hunt.nv,
      }
      const spawnList = composition
        .map(([id, weight]) => ({ species: species.get(id), weight: Math.max(0, weight) }))
        .filter((spawn): spawn is { species: ReferenceSpecies; weight: number } => Boolean(spawn.species) && spawn.weight > 0)
      const totalWeight = spawnList.reduce((total, spawn) => total + spawn.weight, 0)
      if (!totalWeight) return null
      const primarySpecies = spawnList.reduce((primary, spawn) => spawn.weight > primary.weight ? spawn : primary).species
      let weightedKillSeconds = 0
      let weightedLootGold = 0
      let weightedXp = 0
      for (const spawn of spawnList) {
        const wildLevel = effectiveHunt.nv
        const hp = Math.max(24, ((spawn.species.b[0] + 32) * wildLevel / 100) * 12 * 5)
        const dps = dpsAgainst(account, activeSpecies, stats, spawn.species, wildLevel, reference.tipos)
        const dpsPadded = Math.max(1, dps)
        const kill = Math.max(0.9, hp / dpsPadded)
        const ratio = spawn.weight / totalWeight
        weightedKillSeconds += kill * ratio
        weightedXp += gameXpPerKill(wildLevel) * ratio
        const dropValue = (spawn.species.l ?? []).reduce((sum, loot) => sum + loot[1] / 100 * ((loot[2] + loot[3]) / 2) * loot[4], 0)
        weightedLootGold += dropValue * ratio
      }
      const secondsPerKill = weightedKillSeconds + 2.8
      const killsPerHour = Math.min(3600 / secondsPerKill, (effectiveHunt.nv < 20 ? 3600 / 4.2 : 3600 / 3.2))
      const xp = weightedXp * killsPerHour
      const gold = (gameGoldPerKill(effectiveHunt.nv) + weightedLootGold) * killsPerHour
      return {
        hunt: effectiveHunt,
        primarySpecies,
        xpTrainerPerHour: Math.round(xp * rankXpTrainer),
        xpPokemonPerHour: Math.round(xp * rankXpPokemon),
        goldPerHour: Math.round(gold),
        killsPerHour: Math.round(killsPerHour),
        secondsPerKill: Math.round(secondsPerKill * 10) / 10,
      }
    })
    .filter((estimate): estimate is HuntEstimate => estimate !== null)
}

export function readHuntReference(value: unknown): HuntReference | null {
  if (!value || typeof value !== 'object') return null
  const candidate = value as Partial<HuntReference>
  if (!Array.isArray(candidate.especies) || !Array.isArray(candidate.hunts) || !candidate.tipos || typeof candidate.tipos !== 'object') return null
  if (!Object.values(candidate.tipos).every((row) => row && typeof row === 'object'
    && Object.values(row).every((value) => Number.isFinite(value)))) return null
  const species = candidate.especies.filter((item): item is ReferenceSpecies => {
    if (!item || typeof item !== 'object') return false
    const record = item as ReferenceSpecies
    return Number.isFinite(record.id) && typeof record.n === 'string'
      && Array.isArray(record.t) && record.t.every((type) => typeof type === 'string')
      && Array.isArray(record.b) && record.b.length >= 6
      && record.b.slice(0, 6).every(Number.isFinite) && Array.isArray(record.m)
      && record.m.every((move) => Array.isArray(move) && move.length >= 6
        && typeof move[0] === 'string' && typeof move[1] === 'string'
        && [move[2], move[3], move[4], move[5]].every(Number.isFinite))
      && (record.l === undefined || record.l.every((loot) => Array.isArray(loot)
        && typeof loot[0] === 'string' && [loot[1], loot[2], loot[3], loot[4]].every(Number.isFinite)))
  })
  const hunts = candidate.hunts.filter((item): item is ReferenceHunt => {
    if (!item || typeof item !== 'object') return false
    const record = item as ReferenceHunt
    return typeof record.s === 'string' && typeof record.n === 'string'
      && typeof record.a === 'string' && Number.isFinite(record.nv)
      && Array.isArray(record.e) && record.e.every((spawn) => Array.isArray(spawn)
        && spawn.length >= 2 && Number.isFinite(spawn[0]) && Number.isFinite(spawn[1]))
  })
  const looktypes = candidate.looktypes && typeof candidate.looktypes === 'object'
    ? Object.fromEntries(Object.entries(candidate.looktypes)
      .filter(([id, looktype]) => /^\d+$/.test(id) && Number.isSafeInteger(looktype) && Number(looktype) > 1))
    : {}
  const speciesWithLooktypes = species.map((item) => ({
    ...item,
    // The public sprite catalog omits Mega forms; guide IDs use 3000 + the
    // base species ID, so use the normal sprite as a graceful visual fallback.
    looktype: looktypes[String(item.id)] ?? item.looktype
      ?? (item.id >= 3000 && item.id < 4000 ? looktypes[String(item.id - 3000)] : undefined),
  }))
  return species.length && hunts.length
    ? { tipos: candidate.tipos as HuntReference['tipos'], especies: speciesWithLooktypes, hunts, looktypes }
    : null
}
