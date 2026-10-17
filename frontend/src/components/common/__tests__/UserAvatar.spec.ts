import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createApp, h, nextTick, reactive, type App } from 'vue'
import UserAvatar from '../UserAvatar.vue'

let app: App
let root: HTMLDivElement
const props = reactive<{ userId: string | null, version: number | null, name: string | null }>({ userId: 'u1', version: 5, name: 'alice' })
async function flush() { for (let i = 0; i < 6; i++) { await Promise.resolve(); await nextTick() } }

beforeEach(() => {
  Object.assign(props, { userId: 'u1', version: 5, name: 'alice' })
  root = document.createElement('div')
  document.body.append(root)
  app = createApp({ render: () => h(UserAvatar, { ...props, size: 40 }) })
  app.mount(root)
})
afterEach(() => { app.unmount(); root.remove() })

describe('UserAvatar', () => {
  it('shows the versioned picture without leaking a referrer', async () => {
    const img = root.querySelector('img')!
    expect(img.getAttribute('src')).toBe('/api/avatars/u1?v=5')
    expect(img.getAttribute('referrerpolicy')).toBe('no-referrer')
    expect(img.getAttribute('alt')).toBe('alice 的头像')
    expect(root.querySelector<HTMLElement>('.user-avatar')!.style.width).toBe('40px')
  })

  it('shows the initial for people without a picture, and for nameless ones a placeholder', async () => {
    props.version = null
    await flush()
    expect(root.querySelector('img')).toBeNull()
    expect(root.querySelector('.user-avatar')!.textContent?.trim()).toBe('A')
    props.name = ''
    await flush()
    expect(root.querySelector('.user-avatar')!.textContent?.trim()).toBe('?')
    props.name = '张三'
    await flush()
    expect(root.querySelector('.user-avatar')!.textContent?.trim()).toBe('张')
  })

  it('falls back to the initial when the picture fails, and retries for a newer version', async () => {
    root.querySelector('img')!.dispatchEvent(new Event('error'))
    await flush()
    expect(root.querySelector('img')).toBeNull()
    expect(root.querySelector('.user-avatar')!.textContent?.trim()).toBe('A')
    props.version = 6
    await flush()
    expect(root.querySelector('img')!.getAttribute('src')).toBe('/api/avatars/u1?v=6')
  })
})
