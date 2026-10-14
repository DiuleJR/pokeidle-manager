import { describe, expect, it } from 'vitest'
import { kickChannelUrl, twitchChannelUrl } from './urls'

describe('live channel URLs', () => {
  it('builds Twitch URLs on the fixed HTTPS host from valid logins', () => {
    expect(twitchChannelUrl('trainer_01')).toBe('https://www.twitch.tv/trainer_01')
    expect(twitchChannelUrl('A'.repeat(25))).toBe(`https://www.twitch.tv/${'A'.repeat(25)}`)
  })

  it('builds Kick URLs on the fixed HTTPS host from valid slugs', () => {
    expect(kickChannelUrl('diule-jr_01')).toBe('https://kick.com/diule-jr_01')
    expect(kickChannelUrl('a'.repeat(100))).toBe(`https://kick.com/${'a'.repeat(100)}`)
  })

  it.each([
    '',
    ' leading',
    'trailing ',
    '../other',
    '..',
    'name/path',
    'name\\path',
    'https://evil.example/channel',
    'name?next=evil.example',
    'name#fragment',
    'name%2fother',
    'name:443',
    'na.me',
  ])('rejects unsafe Twitch login %j', (login) => {
    expect(twitchChannelUrl(login)).toBeNull()
  })

  it.each([
    '',
    ' leading',
    'trailing ',
    '../other',
    '..',
    'name/path',
    'name\\path',
    'https://evil.example/channel',
    'name?next=evil.example',
    'name#fragment',
    'name%2fother',
    'name:443',
    'na.me',
  ])('rejects unsafe Kick slug %j', (slug) => {
    expect(kickChannelUrl(slug)).toBeNull()
  })

  it('rejects non-string input and overlong identifiers', () => {
    for (const value of [null, undefined, 42, {}, ['channel']]) {
      expect(twitchChannelUrl(value)).toBeNull()
      expect(kickChannelUrl(value)).toBeNull()
    }
    expect(twitchChannelUrl('a'.repeat(26))).toBeNull()
    expect(kickChannelUrl('a'.repeat(101))).toBeNull()
  })
})
