import type {
  AccountDomainRevisions,
  AccountMode,
  AccountStatus,
  AccountView,
  InventoryItem,
} from './types'

export interface IntegrationDiagnostic {
  lifecycle:
    | 'starting'
    | 'connectingCdp'
    | 'loadingGame'
    | 'waitingForLogin'
    | 'waitingForHello'
    | 'waitingForWelcome'
    | 'authenticated'
    | 'readyForHandoff'
    | 'closing'
    | 'closed'
    | 'error'
  sessionMode:
    | 'interactive'
    | 'interactiveOwner'
    | 'reconnectExistingOwner'
    | 'backgroundBootstrap'
    | 'recoveryProbe'
    | null
  braveFound: boolean
  profileCreated: boolean
  accountPersisted: boolean
  persistentProfile: boolean
  braveStarted: boolean
  cdpPort: number | null
  cdpEndpointAvailable: boolean
  cdpEndpointAttempts: number
  cdpEndpointLastError: string | null
  browserProduct: string | null
  browserWsUrlObtained: boolean
  browserWsConnected: boolean
  cdpConnected: boolean
  targetFound: boolean
  targetIdFound: boolean
  sessionCreated: boolean
  managedGamePage: boolean
  pageEnabled: boolean
  pageNavigateSent: boolean
  gameUrlNavigated: boolean
  finalUrl: string | null
  networkEnabled: boolean
  websocketCount: number
  pokeidleSocketDetected: boolean
  websocketDetected: boolean
  helloDetected: boolean
  welcomeReceived: boolean
  wsUrlCaptured: boolean
  sessionMaterialCaptured: boolean
  rustWsConnected: boolean
  rustHelloSent: boolean
  rustWelcomeReceived: boolean
  browserClosed: boolean
  backgroundActive: boolean
  controlledBravePid: number | null
  automaticReloadUsed: boolean
  nick: string | null
  state: string
  message: string
  accountId: string | null
}

interface RuntimePokemon {
  id: number
  nome: string
  level: number
  hp: number
  maxHp: number
  xp?: number
  xp_level?: number
  xp_next?: number
  quality?: number
  potencia?: number
  nota?: number
  shiny?: boolean
  species_id?: number
  looktype?: number
  look_shiny?: number
  types?: string[]
  iv_total?: number
  ivs?: { hp?: number; atk?: number; def?: number; spAtk?: number; spDef?: number; speed?: number }
  held_item_id?: number
}
interface RuntimeAccount {
  id: string
  nick: string
  card_color: string
  status: AccountStatus
  mode: AccountMode
  runtime?: 'offline' | 'browser_bootstrap' | 'waiting_for_login' | 'browser_connected' | 'preparing_handoff' | 'background_connecting' | 'background' | 'reconnecting' | 'login_required' | 'error' | 'stopping'
}
export interface RuntimeSnapshot {
  account: RuntimeAccount
  state: {
    level?: number
    xp_bonus?: { eventTrainerPct?: number; eventPokemonPct?: number; guildRankPct?: number; guildBoostActive?: boolean; twitchPct?: number }
    xp?: number
    gold?: number
    diamonds?: number
    orbs?: number
    vip_until?: number
    vip_active?: boolean
    server_now?: number
    vip_data_available?: boolean
    center_free_at?: number
    combat_lock_until?: number
    server_offset_ms?: number
    combat_locked?: boolean
    command_transport_available?: boolean
    pending_navigation?: { kind: 'hunt'; slug: string } | { kind: 'center' }
    navigation_error?: string
    automation_error?: string
    hunt_slug?: string
    hunt_started_at_ms?: number | null
    pending_hunt_slug?: string
    no_centro: boolean
    activity?: 'farming' | 'pokemon_center' | 'idle' | 'unknown'
    active_id?: number
    items: Record<string, number>
    balls: Record<string, number>
    active_ball_id?: number
    active_potion_id?: number | null
    active_potion_source?: 'inventory_delta'
    hunts?: { slug: string; name: string; area?: string; level?: number; total_spawns?: number; region?: string; looktype?: number; species?: { species_id?: number; weight?: number }[] }[]
    pokemon: RuntimePokemon[]
    wild?: { nome: string; level: number; hp: number; maxHp: number }
    auto_buy_rules?: Array<{ kind: 'item' | 'ball'; item_id: number; minimum: number; quantity: number; enabled: boolean; status?: string }>
    capture_mode?: 'until_capture' | 'continuous' | 'off'
    capture_queue_len?: number
    capture_error?: string
    automation: {
      auto_potion?: boolean
      hp_threshold?: number
      auto_revive?: boolean
      auto_sale_loot?: boolean
      auto_lock_shiny?: boolean
      auto_lock_nota9?: boolean
      auto_lock_nota_min?: number
      auto_lock_p5?: boolean
      auto_return_hunt?: boolean
      potion_ids: number[]
      ball_ids: number[]
      revive_ids: number[]
    }
    hunt_session?: {
      hunt_slug: string
      started_at_ms: number
      kills: number
      captures: number
      xp_obtained: number
      trainer_xp: number
      pokemon_xp: number
      gold_combat: number
      gold_auto_sale: number
      drops: Record<string, number>
      balls_used: Record<string, number>
      shinies_seen: number
      shinies_captured: number
    }
  }
  metrics: { xp_per_hour: number; gold_per_hour: number; kills: number; captures: number; potions_used?: Record<string, number>; potions_per_hour?: number; potion_usage_per_hour?: Record<string, number>; balls_used?: Record<string, number> }
}

export interface IntegrationSnapshot {
  diagnostic: IntegrationDiagnostic
  accounts: RuntimeSnapshot[]
}

export interface LiveRuntimeSnapshot {
  account: RuntimeAccount
  state: Omit<
    RuntimeSnapshot['state'],
    'active_id' | 'active_potion_id' | 'active_ball_id' | 'items' | 'balls' | 'pokemon' | 'hunts'
  > & {
    active_pokemon?: RuntimePokemon | null
    active_hunt?: {
      slug: string
      name: string
      area?: string | null
      region?: string | null
    } | null
    automation: RuntimeSnapshot['state']['automation']
  }
  metrics: RuntimeSnapshot['metrics']
  current_potion_id?: number | null
  current_potion_quantity: number
  current_ball_id?: number | null
  current_ball_quantity: number
  revisions: {
    depot: number
    inventory: number
    hunt_options: number
  }
}

export interface IntegrationLiveSnapshot {
  diagnostic: IntegrationDiagnostic
  accounts: LiveRuntimeSnapshot[]
}

const sum = (values: Record<string, number>) => Object.values(values).reduce((total, value) => total + value, 0)
const ballNames: Record<string, string> = { '1': 'Poké Ball', '2': 'Great Ball', '3': 'Super Ball', '4': 'Ultra Ball', '5': 'Beast Ball' }
const itemNames: Record<string, string> = { '200': 'Small Potion', '201': 'Great Potion', '202': 'Ultra Potion', '203': 'Hyper Potion', '204': 'Ultimate Potion', '70070': 'Golden Potion' }
export const formatHuntSlug = (slug: string) => slug.split('_').filter(Boolean).map((word) => word[0]?.toUpperCase() + word.slice(1)).join(' ')

/**
 * Converts only the live summary DTO. In particular, it never reconstructs or
 * exposes the Pokémon depot, inventory map, or account-specific hunt catalog.
 */
export const accountFromLive = (snapshot: LiveRuntimeSnapshot): AccountView => {
  const { account, state, metrics } = snapshot
  const active = state.active_pokemon ?? undefined
  const onlineSeconds = state.hunt_session
    ? Math.max(0, Math.floor((Date.now() - state.hunt_session.started_at_ms) / 1000))
    : 0
  const hourly = (used: Record<string, number> | undefined) =>
    onlineSeconds > 0 ? Math.round((sum(used ?? {}) * 3600) / onlineSeconds) : 0
  const currentPotionId = snapshot.current_potion_id
  const currentBallId = snapshot.current_ball_id
  const hunt = state.no_centro
    ? 'Pokémon Center'
    : (state.active_hunt?.name ?? (state.hunt_slug ? formatHuntSlug(state.hunt_slug) : null))
  const automation = state.automation
  const autoBuyRules =
    state.auto_buy_rules?.map((rule) => ({
      kind: rule.kind,
      itemId: rule.item_id,
      minimum: rule.minimum,
      quantity: rule.quantity,
      enabled: rule.enabled,
      status: rule.status,
    })) ?? []

  return {
    id: account.id,
    nick: account.nick,
    color: account.card_color,
    status: account.status,
    mode: account.mode,
    runtime: account.runtime,
    domainRevisions: {
      depot: snapshot.revisions.depot,
      inventory: snapshot.revisions.inventory,
      hunt_options: snapshot.revisions.hunt_options,
    } satisfies AccountDomainRevisions,
    level: active?.level ?? state.level ?? 0,
    trainerLevel: state.level ?? 0,
    activePokemon: active
      ? {
          id: String(active.id),
          name: active.nome,
          level: active.level,
          speciesId: active.species_id,
          quality: active.quality,
          potency: active.potencia,
          shiny: active.shiny,
          looktype: active.looktype,
          heldItemId: active.held_item_id,
          lookShiny: active.look_shiny,
          types: active.types ?? [],
          ivs: active.ivs,
        }
      : undefined,
    xpBonus: state.xp_bonus
      ? {
          eventTrainerPct: state.xp_bonus.eventTrainerPct,
          eventPokemonPct: state.xp_bonus.eventPokemonPct,
          guildRankPct: state.xp_bonus.guildRankPct,
          guildBoostActive: state.xp_bonus.guildBoostActive,
          twitchPct: state.xp_bonus.twitchPct,
        }
      : undefined,
    xp: active?.xp ?? state.xp ?? 0,
    xpLevel: active?.xp_level ?? 0,
    xpNext: active?.xp_next ?? 0,
    power: active?.potencia ?? 0,
    quality: active?.quality ?? 0,
    types: active?.types ?? [],
    vip:
      state.vip_active ??
      (state.vip_until !== undefined && state.server_now !== undefined
        ? state.vip_until > state.server_now
        : undefined),
    vipExpiresAt: state.vip_until,
    vipDataAvailable: state.vip_data_available,
    combatLockUntil: state.combat_lock_until ?? state.center_free_at,
    serverOffsetMs: state.server_offset_ms,
    combatLocked:
      state.combat_lock_until !== undefined && state.server_offset_ms !== undefined
        ? Date.now() + state.server_offset_ms < state.combat_lock_until
        : (state.combat_locked ?? false),
    commandTransportAvailable: state.command_transport_available,
    pendingNavigation: state.pending_navigation,
    navigationError: state.navigation_error,
    automationError: state.automation_error,
    autoBuyRules,
    captureMode: state.capture_mode ?? 'off',
    captureQueueLen: state.capture_queue_len ?? 0,
    captureError: state.capture_error,
    huntOptions: [],
    pendingHunt: state.pending_hunt_slug,
    map: state.active_hunt?.area ?? state.active_hunt?.region ?? '—',
    activity: state.activity,
    onlineSeconds,
    huntStartedAtMs: state.hunt_started_at_ms,
    huntSession: state.hunt_session
      ? {
          huntSlug: state.hunt_session.hunt_slug,
          startedAtMs: state.hunt_session.started_at_ms,
          kills: state.hunt_session.kills,
          captures: state.hunt_session.captures,
          xpObtained: state.hunt_session.xp_obtained,
          trainerXp: state.hunt_session.trainer_xp,
          pokemonXp: state.hunt_session.pokemon_xp,
          goldCombat: state.hunt_session.gold_combat,
          goldAutoSale: state.hunt_session.gold_auto_sale,
          drops: state.hunt_session.drops,
          ballsUsed: state.hunt_session.balls_used,
          shiniesSeen: state.hunt_session.shinies_seen,
          shiniesCaptured: state.hunt_session.shinies_captured,
        }
      : null,
    pokemon: active?.nome ?? '—',
    hp: active?.hp ?? 0,
    maxHp: active?.maxHp ?? 0,
    xpPerHour: metrics.xp_per_hour,
    goldPerHour: metrics.gold_per_hour,
    gold: state.gold ?? 0,
    gems: state.orbs ?? 0,
    diamonds: state.diamonds ?? 0,
    potions: snapshot.current_potion_quantity,
    potionName:
      currentPotionId == null
        ? 'Potion não configurada'
        : (itemNames[String(currentPotionId)] ?? `Item #${currentPotionId}`),
    activePotionId: currentPotionId == null ? undefined : String(currentPotionId),
    potionUsageSource: state.active_potion_source,
    balls: snapshot.current_ball_quantity,
    ballName:
      currentBallId == null
        ? 'Ball não configurada'
        : (ballNames[String(currentBallId)] ?? `Ball #${currentBallId}`),
    activeBallId: currentBallId == null ? undefined : `ball-${currentBallId}`,
    hunt,
    wildPokemon: state.wild
      ? {
          name: state.wild.nome,
          level: state.wild.level,
          hp: state.wild.hp,
          maxHp: state.wild.maxHp,
        }
      : null,
    kills: state.hunt_session?.kills ?? metrics.kills,
    captures: state.hunt_session?.captures ?? metrics.captures,
    xpGained: state.hunt_session?.xp_obtained ?? 0,
    goldGained: (state.hunt_session?.gold_combat ?? 0) + (state.hunt_session?.gold_auto_sale ?? 0),
    drops: Object.entries(state.hunt_session?.drops ?? {}).map(
      ([name, quantity]) => `${name} ×${quantity}`,
    ),
    potionsPerHour: metrics.potions_per_hour ?? 0,
    potionUsagePerHour: metrics.potion_usage_per_hour ?? {},
    ballsPerHour: hourly(metrics.balls_used),
    revives: 0,
    otherItems: 0,
    automations: {
      autoPotion: automation.auto_potion ?? false,
      autoRevive: automation.auto_revive ?? false,
      ballUntilCapture: state.capture_mode === 'until_capture',
      ballContinuous: state.capture_mode === 'continuous',
      autoVendaLoot: automation.auto_sale_loot ?? false,
      returnToHunt: automation.auto_return_hunt ?? false,
      autoBuyPotion: autoBuyRules.some((rule) => rule.kind === 'item' && rule.enabled),
      autoBuyBall: autoBuyRules.some((rule) => rule.kind === 'ball' && rule.enabled),
    },
    potionThreshold: Math.min(
      100,
      Math.max(10, Math.round((automation.hp_threshold ?? 0.4) * 100)),
    ),
    potionIds: automation.potion_ids.map(String),
    ballIds: automation.ball_ids.map(String),
    inventory: [],
    depot: [],
    depotLocks: {
      autoLockShiny: automation.auto_lock_shiny ?? false,
      autoLockNota9: automation.auto_lock_nota9 ?? false,
      autoLockNotaMin: automation.auto_lock_nota_min ?? 0,
      autoLockP5: automation.auto_lock_p5 ?? false,
    },
  }
}

export const accountFromRuntime = (snapshot: RuntimeSnapshot): AccountView => {
  const { account, state, metrics } = snapshot
  const active = state.active_id === undefined
    ? undefined
    : state.pokemon.find((pokemon) => pokemon.id === state.active_id)
  // A captured/reconciled use wins. Before one exists (the Rust `Option`
  // serializes as null), retain the established visual fallback: the first
  // configured Potion that currently has stock. This never changes the
  // backend's observed activePotionId or claims a protocol confirmation.
  const configuredPotionId = state.automation.potion_ids.find((id) => (state.items[String(id)] ?? 0) > 0)
  const activePotionId = typeof state.active_potion_id === 'number'
    ? state.active_potion_id
    : configuredPotionId
  const configuredBallId = state.automation.ball_ids.find((id) => (state.balls[String(id)] ?? 0) > 0)
  const activeBallId = state.active_ball_id ?? configuredBallId
  const selectedHunt = state.hunts?.find((hunt) => hunt.slug === state.hunt_slug)
  const onlineSeconds = state.hunt_session ? Math.max(0, Math.floor((Date.now() - state.hunt_session.started_at_ms) / 1000)) : 0
  const hourly = (used: Record<string, number> | undefined) => onlineSeconds > 0
    ? Math.round((sum(used ?? {}) * 3600) / onlineSeconds)
    : 0
  const inventory: InventoryItem[] = [
    ...Object.entries(state.items).map(([id, quantity]) => ({ id, assetKey: id, name: itemNames[id] ?? `Item #${id}`, quantity, category: itemNames[id] ? 'potions' as const : 'other' as const })),
    ...Object.entries(state.balls).map(([id, quantity]) => ({ id: `ball-${id}`, assetKey: `ball-${id}`, name: ballNames[id] ?? `Ball #${id}`, quantity, category: 'balls' as const })),
  ]
  return {
    id: account.id,
    nick: account.nick,
    color: account.card_color,
    status: account.status,
    mode: account.mode,
    runtime: account.runtime,
    level: active?.level ?? state.level ?? 0,
    trainerLevel: state.level ?? 0,
    activePokemon: active ? {
      id: String(active.id), name: active.nome, level: active.level,
      speciesId: active.species_id, quality: active.quality, potency: active.potencia,
      shiny: active.shiny, looktype: active.looktype, heldItemId: active.held_item_id,
      lookShiny: active.look_shiny,
      types: active.types ?? [], ivs: active.ivs,
    } : undefined,
    xpBonus: state.xp_bonus ? {
      eventTrainerPct: state.xp_bonus.eventTrainerPct,
      eventPokemonPct: state.xp_bonus.eventPokemonPct,
      guildRankPct: state.xp_bonus.guildRankPct,
      guildBoostActive: state.xp_bonus.guildBoostActive,
      twitchPct: state.xp_bonus.twitchPct,
    } : undefined,
    xp: active?.xp ?? state.xp ?? 0,
    xpLevel: active?.xp_level ?? 0,
    xpNext: active?.xp_next ?? 0,
    power: active?.potencia ?? 0,
    quality: active?.quality ?? 0,
    types: active?.types ?? [],
    // `estado.loja.vip` is the real account's authoritative flag. The
    // timestamp comparison remains only for older captures that lack it.
    vip:
      state.vip_active ??
      (state.vip_until !== undefined && state.server_now !== undefined
        ? state.vip_until > state.server_now
        : undefined),
    vipExpiresAt: state.vip_until,
    vipDataAvailable: state.vip_data_available,
    combatLockUntil: state.combat_lock_until ?? state.center_free_at,
    serverOffsetMs: state.server_offset_ms,
    combatLocked:
      state.combat_lock_until !== undefined && state.server_offset_ms !== undefined
        ? Date.now() + state.server_offset_ms < state.combat_lock_until
        : state.combat_locked ?? false,
    commandTransportAvailable: state.command_transport_available,
    pendingNavigation: state.pending_navigation,
    navigationError: state.navigation_error,
    automationError: state.automation_error,
    autoBuyRules: state.auto_buy_rules?.map((rule) => ({
      kind: rule.kind, itemId: rule.item_id, minimum: rule.minimum,
      quantity: rule.quantity, enabled: rule.enabled, status: rule.status,
    })) ?? [],
    captureMode: state.capture_mode ?? 'off',
    captureQueueLen: state.capture_queue_len ?? 0,
    captureError: state.capture_error,
    huntOptions: state.hunts?.map((hunt) => ({
      ...hunt,
      species: hunt.species?.map((species) => ({ speciesId: species.species_id, weight: species.weight })),
    })) ?? [],
    pendingHunt: state.pending_hunt_slug,
    map: selectedHunt?.area ?? selectedHunt?.region ?? '—',
    onlineSeconds,
    huntStartedAtMs: state.hunt_started_at_ms,
    huntSession: state.hunt_session
      ? {
          huntSlug: state.hunt_session.hunt_slug,
          startedAtMs: state.hunt_session.started_at_ms,
          kills: state.hunt_session.kills,
          captures: state.hunt_session.captures,
          xpObtained: state.hunt_session.xp_obtained,
          trainerXp: state.hunt_session.trainer_xp,
          pokemonXp: state.hunt_session.pokemon_xp,
          goldCombat: state.hunt_session.gold_combat,
          goldAutoSale: state.hunt_session.gold_auto_sale,
          drops: state.hunt_session.drops,
          ballsUsed: state.hunt_session.balls_used,
          shiniesSeen: state.hunt_session.shinies_seen,
          shiniesCaptured: state.hunt_session.shinies_captured,
        }
      : null,
    pokemon: active?.nome ?? '—',
    hp: active?.hp ?? 0,
    maxHp: active?.maxHp ?? 0,
    xpPerHour: metrics.xp_per_hour,
    goldPerHour: metrics.gold_per_hour,
    gold: state.gold ?? 0,
    gems: state.orbs ?? 0,
    diamonds: state.diamonds ?? 0,
    potions: activePotionId === undefined ? 0 : state.items[String(activePotionId)] ?? 0,
    potionName: activePotionId === undefined ? 'Potion não configurada' : itemNames[String(activePotionId)] ?? `Item #${activePotionId}`,
    potionUsageSource: state.active_potion_source,
    balls: activeBallId === undefined ? 0 : state.balls[String(activeBallId)] ?? 0,
    ballName: activeBallId === undefined ? 'Ball não configurada' : ballNames[String(activeBallId)] ?? `Ball #${activeBallId}`,
    hunt: state.no_centro ? 'Pokémon Center' : selectedHunt?.name ?? (state.hunt_slug ? formatHuntSlug(state.hunt_slug) : null),
    wildPokemon: state.wild
      ? { name: state.wild.nome, level: state.wild.level, hp: state.wild.hp, maxHp: state.wild.maxHp }
      : null,
    kills: state.hunt_session?.kills ?? metrics.kills,
    captures: state.hunt_session?.captures ?? metrics.captures,
    xpGained: state.hunt_session?.xp_obtained ?? 0,
    goldGained: (state.hunt_session?.gold_combat ?? 0) + (state.hunt_session?.gold_auto_sale ?? 0),
    drops: Object.entries(state.hunt_session?.drops ?? {}).map(([name, quantity]) => `${name} ×${quantity}`),
    potionsPerHour: metrics.potions_per_hour ?? 0,
    potionUsagePerHour: metrics.potion_usage_per_hour ?? {},
    ballsPerHour: hourly(metrics.balls_used),
    revives: 0,
    otherItems: sum(state.items),
    automations: {
      autoPotion: state.automation.auto_potion ?? false,
      autoRevive: state.automation.auto_revive ?? false,
      ballUntilCapture: state.capture_mode === 'until_capture',
      ballContinuous: state.capture_mode === 'continuous',
      autoVendaLoot: state.automation.auto_sale_loot ?? false,
      returnToHunt: state.automation.auto_return_hunt ?? false,
      autoBuyPotion: state.auto_buy_rules?.some((rule) => rule.kind === 'item' && rule.enabled) ?? false,
      autoBuyBall: state.auto_buy_rules?.some((rule) => rule.kind === 'ball' && rule.enabled) ?? false,
    },
    potionThreshold: Math.min(100, Math.max(10, Math.round((state.automation.hp_threshold ?? 0.4) * 100))),
    potionIds: state.automation.potion_ids.map(String),
    ballIds: state.automation.ball_ids.map(String),
    inventory,
    depot: state.pokemon.map((pokemon) => ({
      id: String(pokemon.id), name: pokemon.nome, level: pokemon.level, types: pokemon.types ?? [], locked: false,
      power: pokemon.potencia, quality: pokemon.quality, note: pokemon.nota, ivTotal: pokemon.iv_total,
      shiny: pokemon.shiny,
      speciesId: pokemon.species_id,
      looktype: pokemon.looktype,
      lookShiny: pokemon.look_shiny,
    })),
    depotLocks: {
      autoLockShiny: state.automation.auto_lock_shiny ?? false,
      autoLockNota9: state.automation.auto_lock_nota9 ?? false,
      autoLockNotaMin: state.automation.auto_lock_nota_min ?? 0,
      autoLockP5: state.automation.auto_lock_p5 ?? false,
    },
  }
}
