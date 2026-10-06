export function formatHuntElapsed(
  startedAtMs: number | null | undefined,
  nowMs: number,
): string | null {
  if (typeof startedAtMs !== 'number' || !Number.isFinite(startedAtMs)) return null

  const elapsedSeconds = Math.max(0, Math.floor((nowMs - startedAtMs) / 1000))
  const hours = Math.floor(elapsedSeconds / 3600)
  const minutes = Math.floor((elapsedSeconds % 3600) / 60)
  const seconds = elapsedSeconds % 60
  const twoDigits = (value: number) => String(value).padStart(2, '0')

  if (hours > 0) return `${twoDigits(hours)}:${twoDigits(minutes)}:${twoDigits(seconds)}`
  return `${twoDigits(minutes)}:${twoDigits(seconds)}`
}
