import { MAX_AVATAR_BYTES } from '@/api/avatar'

export const AVATAR_SIZE = 256
export const AVATAR_ACCEPT = 'image/png,image/jpeg,image/webp,image/gif,image/avif'

/** Pictures bigger than this are refused before decoding, so a huge file cannot freeze the tab. */
export const MAX_SOURCE_BYTES = 12 * 1024 * 1024

export class AvatarImageError extends Error {}

/** The largest centred square of a `width` x `height` image. */
export function centeredSquare(width: number, height: number): { x: number, y: number, side: number } {
  const side = Math.min(width, height)
  return { x: Math.floor((width - side) / 2), y: Math.floor((height - side) / 2), side }
}

function canvasToBlob(canvas: HTMLCanvasElement, type: string, quality?: number): Promise<Blob | null> {
  return new Promise(resolve => canvas.toBlob(resolve, type, quality))
}

/**
 * Turns whatever the user picked into a small square picture: cropped to the centre, scaled to
 * 256x256 and re-encoded as WebP (PNG where WebP is unavailable). Re-encoding also drops any
 * metadata such as GPS positions in photos.
 */
export async function prepareAvatar(file: File): Promise<Blob> {
  if (!file.type.startsWith('image/') || file.type === 'image/svg+xml') {
    throw new AvatarImageError('请选择 PNG、JPEG、WebP 或 GIF 图片')
  }
  if (file.size > MAX_SOURCE_BYTES) throw new AvatarImageError('图片太大了，请选择 12 MB 以内的图片')
  let bitmap: ImageBitmap
  try {
    bitmap = await createImageBitmap(file)
  } catch {
    throw new AvatarImageError('无法读取这张图片，请换一张')
  }
  try {
    const { x, y, side } = centeredSquare(bitmap.width, bitmap.height)
    const canvas = document.createElement('canvas')
    canvas.width = AVATAR_SIZE
    canvas.height = AVATAR_SIZE
    const context = canvas.getContext('2d')
    if (!context) throw new AvatarImageError('当前浏览器无法处理图片')
    context.drawImage(bitmap, x, y, side, side, 0, 0, AVATAR_SIZE, AVATAR_SIZE)
    for (const [type, quality] of [['image/webp', 0.9], ['image/png', undefined]] as const) {
      const blob = await canvasToBlob(canvas, type, quality)
      // Browsers silently fall back to PNG when they cannot encode WebP, so check the result.
      if (blob && blob.type === type) {
        if (blob.size > MAX_AVATAR_BYTES) throw new AvatarImageError('处理后的图片仍然过大，请换一张更简单的图片')
        return blob
      }
    }
    throw new AvatarImageError('当前浏览器无法处理图片')
  } finally {
    bitmap.close()
  }
}
