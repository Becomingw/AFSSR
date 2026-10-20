import { describe, expect, it } from 'vitest'
import { MIN_CELL_SIZE, computeCellSize, createWidthStabilizer } from '../heatmapSizing'

describe('computeCellSize', () => {
  it('returns integer pixel sizes', () => {
    // (1000 - 52 * 4) / 53 = 14.94...
    expect(computeCellSize(1000, 53, 4)).toBe(14)
    expect(computeCellSize(1000.6, 53, 4)).toBe(14)
  })

  it('never overflows the available width', () => {
    for (let width = 200; width < 2000; width += 7.3) {
      const size = computeCellSize(width, 53, 4)
      if (size > MIN_CELL_SIZE) {
        expect(size * 53 + 52 * 4).toBeLessThanOrEqual(width)
      }
    }
  })

  it('falls back to the minimum size', () => {
    expect(computeCellSize(100, 53, 4)).toBe(MIN_CELL_SIZE)
    expect(computeCellSize(0, 53, 4)).toBe(MIN_CELL_SIZE)
    expect(computeCellSize(500, 0, 4)).toBe(MIN_CELL_SIZE)
  })

  it('is stable under sub-pixel width jitter', () => {
    expect(computeCellSize(1000.2, 53, 4)).toBe(computeCellSize(999.8, 53, 4))
  })
})

describe('createWidthStabilizer', () => {
  it('ignores sub-pixel jitter', () => {
    const s = createWidthStabilizer()
    expect(s.update(1000, 0)).toBe(1000)
    expect(s.update(1000.6, 10)).toBe(1000)
    expect(s.update(999.4, 20)).toBe(1000)
  })

  it('breaks the scrollbar feedback loop', () => {
    const s = createWidthStabilizer()
    s.update(1000, 0)
    // vertical scrollbar appears: width shrinks by 15px
    expect(s.update(985, 16)).toBe(985)
    // scrollbar disappears again: bounce back is ignored
    expect(s.update(1000, 32)).toBe(985)
    expect(s.update(1000, 48)).toBe(985)
  })

  it('applies genuine resizes', () => {
    const s = createWidthStabilizer()
    s.update(1000, 0)
    expect(s.update(985, 16)).toBe(985)
    // large growth (e.g. entering fullscreen) is applied
    expect(s.update(1400, 40)).toBe(1400)
    // growth long after the last shrink is applied
    expect(s.update(1300, 100)).toBe(1300)
    expect(s.update(1315, 5000)).toBe(1315)
  })

  it('always applies shrinking so content fits', () => {
    const s = createWidthStabilizer()
    s.update(1000, 0)
    expect(s.update(700, 1)).toBe(700)
    expect(s.update(690, 2)).toBe(690)
  })
})
