import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App } from 'vue'
import EmailSettings from '../EmailSettings.vue'

const mocks = vi.hoisted(() => ({
  testSmtp: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
  auth: { user: { id: 'u1', email: 'root@example.test' } as { id: string, email?: string } },
}))

vi.mock('@/api/admin', () => ({
  adminApi: {
    getAllSystemConfigs: vi.fn().mockResolvedValue([
      { key: 'smtp_host', value: 'smtp.example.test' },
      { key: 'smtp_port', value: 587 },
      { key: 'smtp_user', value: 'ops@example.test' },
      { key: 'smtp_password', value: null, is_set: true },
      { key: 'smtp_from_email', value: 'ops@example.test' },
    ]),
    getEmailTemplates: vi.fn().mockResolvedValue({ templates: [] }),
    testSmtpConnection: mocks.testSmtp,
    updateSystemConfig: vi.fn(),
  },
}))
vi.mock('@/api/auth', () => ({ authApi: { getRegistrationSettings: vi.fn().mockResolvedValue({ require_email_verification: false, email_configured: true }) } }))
vi.mock('@/stores/auth', () => ({ useAuthStore: () => mocks.auth }))
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ success: mocks.success, error: mocks.error, warning: vi.fn() }) }))

let app: App
let root: HTMLDivElement
async function flush() { for (let i = 0; i < 12; i++) { await Promise.resolve(); await nextTick() } }
const sendButton = () => [...root.querySelectorAll('button')].find(b => b.textContent?.includes('发送测试邮件'))!
const recipient = () => root.querySelector<HTMLInputElement>('input[aria-label="测试邮件收件邮箱"]')!

beforeEach(async () => {
  vi.clearAllMocks()
  mocks.auth.user = { id: 'u1', email: 'root@example.test' }
  root = document.createElement('div')
  document.body.append(root)
  app = createApp(EmailSettings)
  app.mount(root)
  await flush()
})
afterEach(() => { app.unmount(); root.remove() })

describe('EmailSettings: send a real test mail', () => {
  it('prefills the administrator address and sends it as send_to, without leaking a stored password', async () => {
    expect(recipient().value).toBe('root@example.test')
    mocks.testSmtp.mockResolvedValue({ success: true, message: '测试邮件已被 SMTP 服务器接受' })
    sendButton().click()
    await flush()
    expect(mocks.testSmtp).toHaveBeenCalledWith(expect.objectContaining({ send_to: 'root@example.test', smtp_host: 'smtp.example.test' }))
    expect(mocks.testSmtp.mock.calls[0]![0].smtp_password).toBeUndefined()
    expect(mocks.success).toHaveBeenCalledWith('测试邮件已被 SMTP 服务器接受')
  })

  it("shows the server's refusal instead of a generic failure", async () => {
    mocks.testSmtp.mockResolvedValue({ success: false, message: '发送失败：unexpected smtp response 554: rejected' })
    sendButton().click()
    await flush()
    expect(mocks.error).toHaveBeenCalledWith('发送失败：unexpected smtp response 554: rejected', '测试邮件发送失败')
    expect(mocks.success).not.toHaveBeenCalled()
  })

  it('cannot be clicked without a recipient', async () => {
    recipient().value = ''
    recipient().dispatchEvent(new Event('input', { bubbles: true }))
    await flush()
    expect(sendButton().disabled).toBe(true)
    sendButton().click()
    await flush()
    expect(mocks.testSmtp).not.toHaveBeenCalled()
  })
})
