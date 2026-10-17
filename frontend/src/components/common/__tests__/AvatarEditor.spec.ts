import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, reactive, type App } from 'vue'
import AvatarEditor from '../AvatarEditor.vue'

const mocks = vi.hoisted(() => ({
  upload: vi.fn(), remove: vi.fn(), prepare: vi.fn(), success: vi.fn(),
  auth: { user: { id: 'u1', username: 'alice', avatar_version: null as number | null } },
}))
vi.mock('@/api/avatar', async (importOriginal) => ({ ...(await importOriginal<object>()), uploadAvatar: mocks.upload, removeAvatar: mocks.remove }))
vi.mock('@/utils/avatarImage', async (importOriginal) => ({ ...(await importOriginal<object>()), prepareAvatar: mocks.prepare }))
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ success: mocks.success, error: vi.fn() }) }))
vi.mock('@/stores/auth', async () => {
  const { reactive } = await import('vue')
  mocks.auth = reactive(mocks.auth)
  return { useAuthStore: () => mocks.auth }
})

let app: App
let root: HTMLDivElement
async function flush() { for (let i = 0; i < 10; i++) { await Promise.resolve(); await nextTick() } }
const button = (label: string) => [...root.querySelectorAll('button')].find(b => b.textContent?.includes(label))
function choose(file: File) {
  const input = root.querySelector<HTMLInputElement>('input[type="file"]')!
  Object.defineProperty(input, 'files', { value: [file], configurable: true })
  input.dispatchEvent(new Event('change', { bubbles: true }))
}

beforeEach(async () => {
  vi.clearAllMocks()
  mocks.auth.user = reactive({ id: 'u1', username: 'alice', avatar_version: null })
  root = document.createElement('div')
  document.body.append(root)
  app = createApp(AvatarEditor)
  app.mount(root)
  await flush()
})
afterEach(() => { app.unmount(); root.remove() })

describe('AvatarEditor', () => {
  it('resizes the chosen picture in the browser, uploads it and shows it', async () => {
    expect(button('上传头像')).toBeTruthy()
    expect(button('移除头像')).toBeUndefined()
    const prepared = new Blob(['x'], { type: 'image/webp' })
    mocks.prepare.mockResolvedValue(prepared)
    mocks.upload.mockResolvedValue({ version: 1792300000000, url: '/api/avatars/u1?v=1792300000000' })
    const original = new File(['big'], 'me.jpg', { type: 'image/jpeg' })
    choose(original)
    await flush()
    expect(mocks.prepare).toHaveBeenCalledWith(original)
    expect(mocks.upload).toHaveBeenCalledWith(prepared)
    expect(mocks.auth.user.avatar_version).toBe(1792300000000)
    expect(root.querySelector('img')!.getAttribute('src')).toBe('/api/avatars/u1?v=1792300000000')
    expect(mocks.success).toHaveBeenCalledWith('头像已更新')
    expect(button('更换头像')).toBeTruthy()
    expect(button('移除头像')).toBeTruthy()
  })

  it('shows why a picture was refused instead of failing silently', async () => {
    const { AvatarImageError } = await import('@/utils/avatarImage')
    mocks.prepare.mockRejectedValue(new AvatarImageError('请选择 PNG、JPEG、WebP 或 GIF 图片'))
    choose(new File(['x'], 'a.svg', { type: 'image/svg+xml' }))
    await flush()
    expect(root.querySelector('[role="alert"]')!.textContent).toContain('请选择 PNG')
    expect(mocks.upload).not.toHaveBeenCalled()
    mocks.prepare.mockResolvedValue(new Blob(['x'], { type: 'image/webp' }))
    mocks.upload.mockRejectedValue({ response: { status: 413 } })
    choose(new File(['x'], 'a.png', { type: 'image/png' }))
    await flush()
    expect(root.querySelector('[role="alert"]')!.textContent).toContain('太大')
    expect(mocks.auth.user.avatar_version).toBeNull()
    mocks.upload.mockRejectedValue(new Error('offline'))
    choose(new File(['x'], 'a.png', { type: 'image/png' }))
    await flush()
    expect(root.querySelector('[role="alert"]')!.textContent).toContain('保存失败')
  })

  it('removes the picture', async () => {
    mocks.auth.user.avatar_version = 5
    await flush()
    mocks.remove.mockResolvedValue(true)
    button('移除头像')!.click()
    await flush()
    expect(mocks.remove).toHaveBeenCalled()
    expect(mocks.auth.user.avatar_version).toBeNull()
    expect(root.querySelector('img')).toBeNull()
    expect(mocks.success).toHaveBeenCalledWith('头像已移除')
  })
})
