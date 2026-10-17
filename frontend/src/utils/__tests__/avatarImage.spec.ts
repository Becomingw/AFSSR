import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AVATAR_SIZE, AvatarImageError, MAX_SOURCE_BYTES, centeredSquare, prepareAvatar } from '../avatarImage'

describe('centeredSquare', () => {
  it('takes the largest centred square of any shape', () => {
    expect(centeredSquare(400, 200)).toEqual({ x: 100, y: 0, side: 200 })
    expect(centeredSquare(200, 400)).toEqual({ x: 0, y: 100, side: 200 })
    expect(centeredSquare(300, 300)).toEqual({ x: 0, y: 0, side: 300 })
    expect(centeredSquare(301, 100)).toEqual({ x: 100, y: 0, side: 100 })
  })
})

describe('prepareAvatar', () => {
  const draw = vi.fn()
  const close = vi.fn()
  let encoders: Record<string, Blob | null>

  beforeEach(() => {
    draw.mockClear()
    close.mockClear()
    encoders = { 'image/webp': new Blob(['w'], { type: 'image/webp' }), 'image/png': new Blob(['p'], { type: 'image/png' }) }
    vi.stubGlobal('createImageBitmap', vi.fn().mockResolvedValue({ width: 600, height: 400, close }))
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ drawImage: draw } as unknown as CanvasRenderingContext2D)
    vi.spyOn(HTMLCanvasElement.prototype, 'toBlob').mockImplementation((callback, type) => {
      callback(encoders[type ?? 'image/png'] ?? null)
    })
  })
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks() })

  const file = (type: string, size = 10) => new File([new Uint8Array(size)], 'a', { type })

  it('crops the centre, scales to the avatar size and prefers WebP', async () => {
    const blob = await prepareAvatar(file('image/jpeg'))
    expect(blob.type).toBe('image/webp')
    expect(draw).toHaveBeenCalledWith(expect.anything(), 100, 0, 400, 400, 0, 0, AVATAR_SIZE, AVATAR_SIZE)
    expect(close).toHaveBeenCalled()
  })

  it('falls back to PNG when the browser cannot encode WebP', async () => {
    encoders['image/webp'] = new Blob(['p'], { type: 'image/png' }) // what browsers do instead of failing
    expect((await prepareAvatar(file('image/png'))).type).toBe('image/png')
  })

  it('refuses SVG, non-images and oversized files before decoding anything', async () => {
    for (const bad of [file('image/svg+xml'), file('text/plain'), file('', 1), file('image/png', MAX_SOURCE_BYTES + 1)]) {
      await expect(prepareAvatar(bad)).rejects.toBeInstanceOf(AvatarImageError)
    }
    expect(createImageBitmap).not.toHaveBeenCalled()
  })

  it('explains unreadable pictures and results that are still too large', async () => {
    vi.stubGlobal('createImageBitmap', vi.fn().mockRejectedValue(new Error('corrupt')))
    await expect(prepareAvatar(file('image/png'))).rejects.toThrow('无法读取')
    vi.stubGlobal('createImageBitmap', vi.fn().mockResolvedValue({ width: 10, height: 10, close }))
    encoders['image/webp'] = new Blob([new Uint8Array(256 * 1024 + 1)], { type: 'image/webp' })
    await expect(prepareAvatar(file('image/png'))).rejects.toThrow('仍然过大')
  })
})
