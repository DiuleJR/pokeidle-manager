export interface TwitchLive {
  login: string
  name: string
  viewers: number
}

export interface KickLive {
  slug: string
  name: string
  viewers: number
  online: boolean
  active: boolean
}

export interface LiveSnapshot {
  twitch: TwitchLive[]
  kick: KickLive[]
  updatedAt: number | null
  sourceAccountId: string | null
  sourceAccountNick: string | null
  sourceTransport: 'browser' | 'background' | 'transition' | 'none' | null
  stale: boolean
}

export const emptyLiveSnapshot: LiveSnapshot = {
  twitch: [],
  kick: [],
  updatedAt: null,
  sourceAccountId: null,
  sourceAccountNick: null,
  sourceTransport: null,
  stale: false,
}

export function sameLiveSnapshot(left: LiveSnapshot, right: LiveSnapshot): boolean {
  return (
    left.updatedAt === right.updatedAt &&
    left.sourceAccountId === right.sourceAccountId &&
    left.sourceAccountNick === right.sourceAccountNick &&
    left.sourceTransport === right.sourceTransport &&
    left.stale === right.stale &&
    left.twitch.length === right.twitch.length &&
    left.twitch.every((live, index) => {
      const next = right.twitch[index]
      return next != null && live.login === next.login && live.name === next.name && live.viewers === next.viewers
    }) &&
    left.kick.length === right.kick.length &&
    left.kick.every((live, index) => {
      const next = right.kick[index]
      return (
        next != null &&
        live.slug === next.slug &&
        live.name === next.name &&
        live.viewers === next.viewers &&
        live.online === next.online &&
        live.active === next.active
      )
    })
  )
}
