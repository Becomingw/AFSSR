import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import Feedback from '../Feedback.vue'
import { canAccessAdminPage } from '@/config/adminRoles'

const mocks = vi.hoisted(() => ({ auth: { user: { id: 'user-1', role: 'user' } }, list: vi.fn(), detail: vi.fn(), create: vi.fn(), command: vi.fn(), push: vi.fn(), route: { path: '/dashboard/feedback', params: {} as Record<string, string>, query: {} as Record<string, string> } }))
vi.mock('@/stores/auth', async () => {
  const { reactive } = await import('vue')
  mocks.auth = reactive(mocks.auth)
  return { useAuthStore: () => mocks.auth }
})
vi.mock('vue-router', async () => {
  const { reactive } = await import('vue')
  mocks.route = reactive(mocks.route)
  mocks.push.mockImplementation(async (path: string) => { mocks.route.path = path; mocks.route.params = { issueId: path.split('/')[3] || '' } })
  return { useRoute: () => mocks.route, useRouter: () => ({ push: mocks.push }) }
})
vi.mock('@/api/feedback', () => ({ listFeedback: mocks.list, getFeedback: mocks.detail, createFeedback: mocks.create, commandFeedback: mocks.command }))
vi.mock('@/components/ui', () => ({
  Button: defineComponent({ setup(_, { slots, attrs }) { return () => h('button', attrs, slots.default?.()) } }),
  Input: defineComponent({ props: { modelValue: { type: [String, Number], default: '' } }, emits: ['update:modelValue'], setup(props, { emit, attrs }) { return () => h('input', { ...attrs, value: props.modelValue, onInput: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).value) }) } }),
}))
vi.mock('@/components/ui/dialog/Dialog.vue', () => ({ default: defineComponent({ props: { open: Boolean, title: String }, setup(props, { slots }) { return () => props.open ? h('section', { 'data-dialog': props.title }, slots.default?.()) : null } }) }))
let app: App, root: HTMLDivElement
const issue = { id: 'issue-id', owner_id: 'user-1', owner_username: 'Researcher', title: '<img src=x onerror=alert(1)>', category: 'model', model: 'fixture', request_id: null, status: 'open', version: 3, activity_seq: 7, created_at: 1700000000, updated_at: 1700000000, description: '<script>bad()</script>' }
async function flush() { for (let i = 0; i < 12; i++) { await Promise.resolve(); await nextTick() } }
function click(label: string) { const button = [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.trim() === label); expect(button).toBeTruthy(); button!.click() }
function fill(element: HTMLInputElement | HTMLTextAreaElement, value: string) { element.value = value; element.dispatchEvent(new Event('input', { bubbles: true })) }
beforeEach(async () => {
  vi.clearAllMocks(); mocks.auth.user = { id: 'user-1', role: 'user' }; mocks.route.path = '/dashboard/feedback/issue-id'; mocks.route.params = { issueId: 'issue-id' }; mocks.route.query = {}
  mocks.list.mockResolvedValue({ items: [issue], total: 1, next_cursor: 7 })
  mocks.detail.mockResolvedValue({ issue, events: [], next_event_cursor: null, has_more: false })
  mocks.create.mockResolvedValue({ issue, replayed: false }); mocks.command.mockResolvedValue({ issue: { ...issue, version: 4, activity_seq: 8 }, replayed: false })
  root = document.createElement('div'); document.body.append(root); app = createApp(Feedback); app.mount(root); await flush()
})
afterEach(() => { app.unmount(); root.remove() })
describe('feedback workflows', () => {
  it('prefills a request handoff without sending history or automatically creating feedback', async () => {
    mocks.route.query = { request_id: 'req-fixture' }; await flush()
    expect(mocks.create).not.toHaveBeenCalled()
    const dialog = root.querySelector('[data-dialog="提交反馈"]')!
    expect(dialog).toBeTruthy()
    fill(dialog.querySelector<HTMLInputElement>('input')!, 'Fixture request failed')
    fill(dialog.querySelector<HTMLTextAreaElement>('textarea')!, 'Synthetic report'); await flush(); click('提交'); await flush()
    expect(mocks.create).toHaveBeenCalledWith(expect.objectContaining({ request_id: 'req-fixture', include_diagnostics: true, description: 'Synthetic report' }), false)
    expect(Object.keys(mocks.create.mock.calls[0]![0])).not.toContain('history')
  })
  it('only associates a reply diagnostic after explicit selection', async () => {
    mocks.route.path = '/admin/feedback/issue-id'; await flush()
    fill(root.querySelector<HTMLInputElement>('input[aria-label="诊断请求 ID"]')!, 'req-fixture'); await flush()
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Reviewed fixture'); await flush()
    const checkbox = root.querySelector<HTMLInputElement>('input[type="checkbox"]')!; checkbox.checked = true; checkbox.dispatchEvent(new Event('change', { bubbles: true })); await flush()
    click('发送回复'); await flush()
    expect(mocks.command).toHaveBeenCalledWith(true, 'issue-id', expect.objectContaining({ diagnostic_request_id: 'req-fixture', content: 'Reviewed fixture' }))
  })
  it('uses own routes for users and renders reports as text, not executable HTML', () => {
    expect(mocks.list).toHaveBeenCalledWith(false, expect.anything())
    expect(root.querySelector('script')).toBeNull(); expect(root.querySelector('img')).toBeNull()
    expect(root.querySelector('[data-testid="feedback-description"]')?.textContent).toBe('<script>bad()</script>')
    expect(root.textContent).toContain('提交反馈')
  })
  it('all administrator roles can create their own feedback through the administrative route', async () => {
    for (const role of ['admin', 'audit_admin', 'user_admin', 'service_admin']) expect(canAccessAdminPage(role, '/admin/feedback/issue-id')).toBe(true)
    for (const role of ['admin', 'audit_admin', 'user_admin', 'service_admin']) {
      mocks.auth.user.role = role; mocks.route.path = '/admin/feedback/issue-id'; await flush()
      expect(mocks.list).toHaveBeenLastCalledWith(true, expect.anything())
      click('提交反馈'); await flush()
      const dialog = root.querySelector('[data-dialog="提交反馈"]')!
      expect(dialog.querySelector<HTMLSelectElement>('[aria-label="反馈可见范围"]')!.value).toBe('private')
      fill(dialog.querySelector<HTMLInputElement>('input')!, `Report from ${role}`)
      fill(dialog.querySelector<HTMLTextAreaElement>('textarea')!, 'Synthetic administrator report')
      await flush(); click('提交'); await flush()
      expect(mocks.create).toHaveBeenLastCalledWith(expect.objectContaining({ title: `Report from ${role}`, visibility: 'private' }), true)
      expect(root.textContent).toContain('标记已解决')
    }
  })
  it('sends replies with a version and keeps the same UUID on a network retry', async () => {
    mocks.command.mockRejectedValueOnce(new Error('network'))
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Synthetic reply'); await flush(); click('发送回复'); await flush()
    const first = mocks.command.mock.calls[0]![2]
    expect(first).toMatchObject({ expected_version: 3, action: 'reply', content: 'Synthetic reply' }); expect(first.client_request_id).toMatch(/^[a-f0-9-]{36}$/)
    click('发送回复'); await flush()
    expect(mocks.command.mock.calls[1]![2].client_request_id).toBe(first.client_request_id)
  })
  it('also allows an administrator to create feedback on the user route', async () => {
    mocks.auth.user.role = 'service_admin'; await flush()
    expect(root.textContent).toContain('提交反馈')
    mocks.route.query = { request_id: 'req-fixture' }; await flush()
    expect(root.querySelector('[data-dialog="提交反馈"]')).toBeTruthy()
  })
  it('does not offer creation to an unknown role or a user on an administrative route', async () => {
    mocks.auth.user.role = 'unknown'; await flush()
    expect(root.textContent).not.toContain('提交反馈')
    mocks.auth.user.role = 'user'; mocks.route.path = '/admin/feedback/issue-id'; await flush()
    expect(root.textContent).not.toContain('提交反馈')
  })
  it('only administrators can close, with explicit confirmation', async () => {
    expect(root.textContent).not.toContain('关闭反馈')
    mocks.auth.user.role = 'audit_admin'; mocks.route.path = '/admin/feedback/issue-id'; await flush()
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Unsent comment'); await flush()
    click('关闭反馈'); await flush(); expect(mocks.command).not.toHaveBeenCalled(); click('确认关闭'); await flush()
    expect(mocks.command).toHaveBeenCalledWith(true, 'issue-id', expect.objectContaining({ action: 'set_status', status: 'closed', expected_version: 3 }))
    expect(mocks.command.mock.calls[0]![2].comment).toBeUndefined()
    expect(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!.value).toBe('Unsent comment')
  })
  it('does not overwrite newer feedback after a conflict or discard the reply draft on refresh', async () => {
    mocks.command.mockRejectedValue({ response: { status: 409 } })
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Keep this draft'); await flush(); click('发送回复'); await flush()
    expect(root.querySelector('[role="alert"]')?.textContent).toContain('未覆盖')
    root.querySelector<HTMLButtonElement>('button[title="刷新反馈"]')!.click(); await flush()
    expect(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!.value).toBe('Keep this draft')
  })
  it('keeps the list cursor separate when appending the newest reply', async () => {
    mocks.detail.mockResolvedValue({ issue, events: [{ seq: 1, kind: 'created', actor_role: 'user', created_at: 1700000000 }], next_event_cursor: 1, has_more: true })
    root.querySelector<HTMLButtonElement>('button[title="刷新反馈"]')!.click(); await flush()
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Reply'); await flush(); click('发送回复'); await flush()
    click('加载更多历史'); await flush()
    expect(mocks.detail).toHaveBeenLastCalledWith(false, 'issue-id', 1)
  })
  it('requires a final comment and submits resolution atomically', async () => {
    const solve = [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.trim() === '评论并标记已解决')!
    expect(solve.disabled).toBe(true); solve.click(); await flush(); expect(mocks.command).not.toHaveBeenCalled()
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Verified fixed'); await flush(); solve.click(); await flush()
    expect(mocks.command).toHaveBeenCalledExactlyOnceWith(false, 'issue-id', expect.objectContaining({ action: 'set_status', status: 'resolved', comment: 'Verified fixed', visibility: 'private' }))
  })
  it('allows other users to comment on public feedback, but not manage its lifecycle', async () => {
    mocks.auth.user.id = 'another-user'
    mocks.command.mockResolvedValue({ issue: { ...issue, visibility: 'public', version: 4, activity_seq: 8 }, replayed: false })
    mocks.detail.mockResolvedValue({ issue: { ...issue, visibility: 'public' }, events: [], has_more: false, next_event_cursor: null })
    root.querySelector<HTMLButtonElement>('button[title="刷新反馈"]')!.click(); await flush()
    expect(root.textContent).not.toContain('评论并标记已解决'); expect(root.textContent).not.toContain('关闭反馈'); expect(root.textContent).not.toContain('重新开启')
    const visibility = root.querySelector<HTMLSelectElement>('[aria-label="回复可见范围"]')!
    expect(visibility.value).toBe('private'); visibility.value = 'public'; visibility.dispatchEvent(new Event('change', { bubbles: true }))
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Public suggestion'); await flush(); click('发送回复'); await flush()
    expect(mocks.command).toHaveBeenCalledWith(false, 'issue-id', expect.objectContaining({ action: 'reply', visibility: 'public' }))
    expect(root.querySelector<HTMLSelectElement>('[aria-label="回复可见范围"]')!.value).toBe('private')
  })
  it('defaults new feedback to private and allows explicit public selection', async () => {
    click('提交反馈'); await flush()
    const dialog = root.querySelector('[data-dialog="提交反馈"]')!
    const visibility = dialog.querySelector<HTMLSelectElement>('[aria-label="反馈可见范围"]')!
    expect(visibility.value).toBe('private')
    visibility.value = 'public'; visibility.dispatchEvent(new Event('change', { bubbles: true }))
    fill(dialog.querySelector<HTMLInputElement>('input')!, 'Share issue'); fill(dialog.querySelector<HTMLTextAreaElement>('textarea')!, 'Safe description'); await flush(); click('提交'); await flush()
    expect(mocks.create).toHaveBeenCalledWith(expect.objectContaining({ visibility: 'public', include_diagnostics: false }), false)
  })
  it('replies to a private comment privately without copying its content', async () => {
    mocks.detail.mockResolvedValue({ issue: { ...issue, visibility: 'public' }, events: [{ seq: 8, kind: 'reply', actor_id: 'user-2', actor_username: 'Participant', actor_role: 'user', visibility: 'private', content: 'Private detail', created_at: 1700000000 }], has_more: false, next_event_cursor: null })
    root.querySelector<HTMLButtonElement>('button[title="刷新反馈"]')!.click(); await flush()
    const visibility = root.querySelector<HTMLSelectElement>('[aria-label="回复可见范围"]')!; visibility.value = 'public'; visibility.dispatchEvent(new Event('change', { bubbles: true })); await flush()
    root.querySelector<HTMLButtonElement>('button[aria-label="回复 Participant"]')!.click(); await flush()
    expect(visibility.value).toBe('private'); expect(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!.value).toBe('@Participant ')
    expect(document.activeElement?.id).toBe('feedback-reply')
  })
  it('renders a redacted private solution without inventing its content', async () => {
    mocks.detail.mockResolvedValue({ issue: { ...issue, visibility: 'public', status: 'resolved' }, events: [{ seq: 8, kind: 'status', actor_username: 'Manager', actor_role: 'admin', visibility: 'private', content: null, from_status: 'open', to_status: 'resolved', created_at: 1700000000 }], has_more: false, next_event_cursor: null })
    root.querySelector<HTMLButtonElement>('button[title="刷新反馈"]')!.click(); await flush()
    expect(root.textContent).toContain('解决说明仅对相关参与者可见')
  })
  it('separates the issue list from the discussion and preserves back navigation', async () => {
    expect(root.querySelector('[aria-label="反馈列表"]')).toBeNull()
    click('所有反馈'); await flush()
    expect(root.querySelector('[aria-label="反馈列表"]')).toBeTruthy(); expect(root.querySelector('#feedback-reply')).toBeNull()
    expect(root.querySelector('[aria-label="反馈状态"]')).toBeTruthy()
  })
})
