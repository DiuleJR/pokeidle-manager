const TWITCH_LOGIN_PATTERN = /^[A-Za-z0-9_]{1,25}$/
const KICK_SLUG_PATTERN = /^[A-Za-z0-9_-]{1,100}$/

/** Build a fixed Twitch channel URL only from a plain Twitch login. */
export function twitchChannelUrl(login: unknown): string | null {
  if (typeof login !== 'string' || !TWITCH_LOGIN_PATTERN.test(login)) return null
  return `https://www.twitch.tv/${login}`
}

/** Build a fixed Kick channel URL only from a plain channel slug. */
export function kickChannelUrl(slug: unknown): string | null {
  if (typeof slug !== 'string' || !KICK_SLUG_PATTERN.test(slug)) return null
  return `https://kick.com/${slug}`
}
