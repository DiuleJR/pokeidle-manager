import { describe, expect, it } from 'vitest'
import { formatHuntElapsed } from './dashboard-time'

describe('formatHuntElapsed', () => {
  const formatFromSeconds = (seconds: number) => formatHuntElapsed(0, seconds * 1000)

  it.each([
    [0, '00:00'],
    [5, '00:05'],
    [12, '00:12'],
    [65, '01:05'],
    [3599, '59:59'],
    [3600, '01:00:00'],
    [5979, '01:39:39'],
    [24 * 60 * 60, '24:00:00'],
    [103 * 60 * 60 + 22 * 60 + 10, '103:22:10'],
  ])('formats %i seconds as %s', (seconds, expected) => {
    expect(formatFromSeconds(seconds)).toBe(expected)
  })

  it('does not show a misleading value before a start timestamp exists', () => {
    expect(formatHuntElapsed(undefined, 10_000)).toBeNull()
    expect(formatHuntElapsed(null, 10_000)).toBeNull()
    expect(formatHuntElapsed(Number.NaN, 10_000)).toBeNull()
  })

  it('clamps a future timestamp to zero elapsed time', () => {
    expect(formatHuntElapsed(10_000, 5_000)).toBe('00:00')
  })
})
