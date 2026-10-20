/**
 * 热力图格子尺寸计算与宽度防抖。
 *
 * 背景：格子是正方形，宽度变化会带动高度变化。在浏览器全屏等视口高度临界的场景下，
 * 高度变化会让页面纵向滚动条出现/消失，进而让容器宽度变化约一个滚动条宽度，
 * 形成 宽度 → 高度 → 滚动条 → 宽度 的反馈环，表现为热力图闪烁（#1）。
 */

export const MIN_CELL_SIZE = 6

/** 计算整数像素的格子边长，避免亚像素尺寸造成的横向溢出与布局抖动。 */
export function computeCellSize(width: number, columnCount: number, gap: number): number {
  if (!Number.isFinite(width) || width <= 0 || columnCount <= 0) {
    return MIN_CELL_SIZE
  }
  const totalGap = Math.max(columnCount - 1, 0) * gap
  const availableSpace = Math.max(width - totalGap, 0)
  return Math.max(MIN_CELL_SIZE, Math.floor(availableSpace / columnCount))
}

export interface WidthStabilizerOptions {
  /** 小于等于该值的变化视为亚像素抖动，直接忽略。 */
  jitter?: number
  /** 收缩后短时间内“回弹”不超过该值时视为滚动条振荡（应覆盖常见滚动条宽度）。 */
  oscillationTolerance?: number
  /** 判定振荡的时间窗口（毫秒）。 */
  oscillationWindowMs?: number
}

/**
 * 创建宽度稳定器：收缩总是立即生效（保证内容放得下），
 * 但刚收缩后又回弹到原宽度的变化会被忽略，从而打破滚动条反馈环。
 */
export function createWidthStabilizer(options: WidthStabilizerOptions = {}) {
  const jitter = options.jitter ?? 1
  const tolerance = options.oscillationTolerance ?? 24
  const windowMs = options.oscillationWindowMs ?? 800
  let current: number | null = null
  let lastShrink: { from: number; at: number } | null = null

  return {
    update(width: number, now: number = Date.now()): number {
      if (current === null) {
        current = width
        return current
      }
      const delta = width - current
      if (Math.abs(delta) <= jitter) {
        return current
      }
      if (delta < 0) {
        lastShrink = { from: current, at: now }
        current = width
        return current
      }
      const isOscillation = lastShrink !== null
        && now - lastShrink.at <= windowMs
        && delta <= tolerance
        && Math.abs(width - lastShrink.from) <= jitter
      if (isOscillation) {
        return current
      }
      lastShrink = null
      current = width
      return current
    },
    reset() {
      current = null
      lastShrink = null
    },
  }
}
