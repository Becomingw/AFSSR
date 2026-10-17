import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import Feedback from '../Feedback.vue'
import { canAccessAdminPage } from '@/config/adminRoles'

const mocks = vi.hoisted(() => ({ auth: { user: { id: 'user-1', role: 'user', username: undefined as string | undefined } as { id: string, role: string, username?: string } }, list: vi.fn(), detail: vi.fn(), create: vi.fn(), command: vi.fn(), mentionable: vi.fn(), push: vi.fn(), route: { path: '/dashboard/feedback', params: {} as Record<string, string>, query: {} as Record<string, string> } }))
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
vi.mock('@/api/feedback', () => ({ listFeedback: mocks.list, getFeedback: mocks.detail, createFeedback: mocks.create, commandFeedback: mocks.command, listMentionableAdmins: mocks.mentionable }))
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
  vi.clearAllMocks(); mocks.auth.user = { id: 'user-1', role: 'user', username: undefined }; mocks.route.path = '/dashboard/feedback/issue-id'; mocks.route.params = { issueId: 'issue-id' }; mocks.route.query = {}
  mocks.list.mockResolvedValue({ items: [issue], total: 1, next_cursor: 7 })
  mocks.detail.mockResolvedValue({ issue, events: [], next_event_cursor: null, has_more: false })
  mocks.mentionable.mockResolvedValue(['root', 'Alice']); mocks.create.mockResolvedValue({ issue, replayed: false }); mocks.command.mockResolvedValue({ issue: { ...issue, version: 4, activity_seq: 8 }, replayed: false })
  root = document.createElement('div'); document.body.append(root); app = createApp(Feedback); app.mount(root); await flush()
})
afterEach(() => { app.unmount(); root.remove() })
describe('feedback workflows', () => {
  it('prefills a request handoff without sending history or automatically creating feedback', async () => {
    mocks.route.query = { request_id: 'req-fixture' }; await flush()
    expect(mocks.create).not.toHaveBeenCalled()
    const dialog = root.querySelector('[data-dialog="发起主题"]')!
    expect(dialog).toBeTruthy()
    fill(dialog.querySelector<HTMLInputElement>('input')!, 'Fixture request failed')
    fill(dialog.querySelector<HTMLTextAreaElement>('textarea')!, 'Synthetic report'); await flush(); click('发起'); await flush()
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
    expect(root.textContent).toContain('发起主题')
  })
  it('all administrator roles can create their own feedback through the administrative route', async () => {
    for (const role of ['admin', 'audit_admin', 'user_admin', 'service_admin']) expect(canAccessAdminPage(role, '/admin/feedback/issue-id')).toBe(true)
    for (const role of ['admin', 'audit_admin', 'user_admin', 'service_admin']) {
      mocks.auth.user.role = role; mocks.route.path = '/admin/feedback/issue-id'; await flush()
      expect(mocks.list).toHaveBeenLastCalledWith(true, expect.anything())
      click('发起主题'); await flush()
      const dialog = root.querySelector('[data-dialog="发起主题"]')!
      expect(dialog.querySelector('[aria-label="反馈可见范围"]')).toBeNull()
      fill(dialog.querySelector<HTMLInputElement>('input')!, `Report from ${role}`)
      fill(dialog.querySelector<HTMLTextAreaElement>('textarea')!, 'Synthetic administrator report')
      await flush(); click('发起'); await flush()
      expect(mocks.create).toHaveBeenLastCalledWith(expect.objectContaining({ title: `Report from ${role}` }), true)
      expect(mocks.create.mock.lastCall![0]).not.toHaveProperty('visibility')
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
    expect(root.textContent).toContain('发起主题')
    mocks.route.query = { request_id: 'req-fixture' }; await flush()
    expect(root.querySelector('[data-dialog="发起主题"]')).toBeTruthy()
  })
  it('does not offer creation to an unknown role or a user on an administrative route', async () => {
    mocks.auth.user.role = 'unknown'; await flush()
    expect(root.textContent).not.toContain('发起主题')
    mocks.auth.user.role = 'user'; mocks.route.path = '/admin/feedback/issue-id'; await flush()
    expect(root.textContent).not.toContain('发起主题')
  })
  it('only administrators can close, with explicit confirmation', async () => {
    expect(root.textContent).not.toContain('关闭主题')
    mocks.auth.user.role = 'audit_admin'; mocks.route.path = '/admin/feedback/issue-id'; await flush()
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Unsent comment'); await flush()
    click('关闭主题'); await flush(); expect(mocks.command).not.toHaveBeenCalled(); click('确认关闭'); await flush()
    expect(mocks.command).toHaveBeenCalledWith(true, 'issue-id', expect.objectContaining({ action: 'set_status', status: 'closed', expected_version: 3 }))
    expect(mocks.command.mock.calls[0]![2].comment).toBeUndefined()
    expect(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!.value).toBe('Unsent comment')
  })
  it('does not overwrite newer feedback after a conflict or discard the reply draft on refresh', async () => {
    mocks.command.mockRejectedValue({ response: { status: 409 } })
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Keep this draft'); await flush(); click('发送回复'); await flush()
    expect(root.querySelector('[role="alert"]')?.textContent).toContain('未覆盖')
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    expect(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!.value).toBe('Keep this draft')
  })
  it('keeps the list cursor separate when appending the newest reply', async () => {
    mocks.detail.mockResolvedValue({ issue, events: [{ seq: 1, kind: 'created', actor_role: 'user', created_at: 1700000000 }], next_event_cursor: 1, has_more: true })
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Reply'); await flush(); click('发送回复'); await flush()
    click('加载更多历史'); await flush()
    expect(mocks.detail).toHaveBeenLastCalledWith(false, 'issue-id', 1)
  })
  it('requires a final comment and submits resolution atomically', async () => {
    const solve = [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.trim() === '评论并标记已解决')!
    expect(solve.disabled).toBe(true); solve.click(); await flush(); expect(mocks.command).not.toHaveBeenCalled()
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Verified fixed'); await flush(); solve.click(); await flush()
    expect(mocks.command).toHaveBeenCalledExactlyOnceWith(false, 'issue-id', expect.objectContaining({ action: 'set_status', status: 'resolved', comment: 'Verified fixed' }))
  })
  it('allows other users to comment on public feedback, but not manage its lifecycle', async () => {
    mocks.auth.user.id = 'another-user'
    mocks.command.mockResolvedValue({ issue: { ...issue, visibility: 'public', version: 4, activity_seq: 8 }, replayed: false })
    mocks.detail.mockResolvedValue({ issue: { ...issue, visibility: 'public' }, events: [], has_more: false, next_event_cursor: null })
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    expect(root.textContent).not.toContain('评论并标记已解决'); expect(root.textContent).not.toContain('关闭主题'); expect(root.textContent).not.toContain('重新开启')
    // There is no visibility choice any more: every reply is public and nothing asks for "private".
    expect(root.querySelector('[aria-label="回复可见范围"]')).toBeNull()
    expect(root.textContent).toContain('所有回复都是公开的')
    fill(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!, 'Public suggestion'); await flush(); click('发送回复'); await flush()
    expect(mocks.command).toHaveBeenCalledWith(false, 'issue-id', expect.objectContaining({ action: 'reply', content: 'Public suggestion' }))
    expect(mocks.command.mock.lastCall![2]).not.toHaveProperty('visibility')
  })
  describe('editing and deleting', () => {
    const reply = (seq: number, actor: string, content: string | null, extra: Record<string, unknown> = {}) => ({ seq, issue_id: 'issue-id', actor_id: actor, actor_username: actor, actor_role: 'user', kind: 'reply', content, from_status: null, to_status: null, created_at: 1700000100 + seq, visibility: 'private', ...extra })
    const withReplies = async (events: unknown[], user = { id: 'user-1', role: 'user' }) => {
      mocks.auth.user = user
      mocks.detail.mockResolvedValue({ issue, events, next_event_cursor: 9, has_more: false })
      root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    }
    const labels = () => [...root.querySelectorAll('button')].map(b => b.getAttribute('aria-label') || b.textContent?.trim())

    it('offers edit and delete only where the server would allow them', async () => {
      await withReplies([reply(2, 'user-1', 'mine'), reply(3, 'someone-else', 'theirs')])
      // The owner edits the issue and their own reply; deletes their own reply but not another user's.
      expect(labels()).toContain('编辑主题')
      expect(labels()).toContain('编辑 user-1 的回复')
      expect(labels()).toContain('删除 user-1 的回复')
      expect(labels()).not.toContain('编辑 someone-else 的回复')
      expect(labels()).not.toContain('删除 someone-else 的回复')
      expect(root.textContent).toContain('删除主题')
      // A bystander on a public issue sees neither issue nor foreign-reply controls.
      mocks.detail.mockResolvedValue({ issue: { ...issue, visibility: 'public' }, events: [reply(3, 'someone-else', 'theirs')], next_event_cursor: 3, has_more: false })
      mocks.auth.user = { id: 'bystander', role: 'user' }
      root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
      expect(labels()).not.toContain('编辑主题'); expect(labels()).not.toContain('删除 someone-else 的回复'); expect(root.textContent).not.toContain('删除主题')
      // Administrators moderate: they may delete any reply, but only authors edit replies.
      mocks.auth.user = { id: 'root', role: 'admin' }; mocks.route.path = '/admin/feedback/issue-id'; await flush()
      expect(labels()).toContain('删除 someone-else 的回复'); expect(labels()).not.toContain('编辑 someone-else 的回复'); expect(labels()).toContain('编辑主题')
    })

    it('edits a reply in place and shows it as edited without refetching old events', async () => {
      await withReplies([reply(2, 'user-1', 'before')])
      root.querySelector<HTMLButtonElement>('button[aria-label="编辑 user-1 的回复"]')!.click(); await flush()
      const form = root.querySelector('form[aria-label="编辑回复"]')!
      const save = [...form.querySelectorAll('button')].find(b => b.textContent?.trim() === '保存')!
      expect(save.disabled).toBe(true) // unchanged text is not a change
      fill(form.querySelector<HTMLTextAreaElement>('textarea')!, 'after'); await flush()
      mocks.command.mockResolvedValue({ issue: { ...issue, version: 4, activity_seq: 9 }, replayed: false })
      mocks.detail.mockResolvedValue({ issue, events: [], next_event_cursor: 9, has_more: false })
      save.click(); await flush()
      expect(mocks.command).toHaveBeenCalledWith(false, 'issue-id', expect.objectContaining({ action: 'edit_reply', seq: 2, content: 'after', expected_version: 3 }))
      expect(root.querySelector('form[aria-label="编辑回复"]')).toBeNull()
      expect(root.textContent).toContain('after'); expect(root.textContent).toContain('已编辑'); expect(root.textContent).not.toContain('before')
    })

    it('asks for confirmation before deleting a reply and leaves a tombstone', async () => {
      await withReplies([reply(2, 'user-1', 'secret words')])
      root.querySelector<HTMLButtonElement>('button[aria-label="删除 user-1 的回复"]')!.click(); await flush()
      expect(mocks.command).not.toHaveBeenCalled()
      mocks.command.mockResolvedValue({ issue: { ...issue, version: 4, activity_seq: 9 }, replayed: false })
      mocks.detail.mockResolvedValue({ issue, events: [], next_event_cursor: 9, has_more: false })
      click('确认删除'); await flush()
      expect(mocks.command).toHaveBeenCalledWith(false, 'issue-id', expect.objectContaining({ action: 'delete_reply', seq: 2 }))
      expect(root.querySelector('[data-testid="deleted-reply"]')?.textContent).toContain('此回复已删除')
      expect(root.textContent).not.toContain('secret words')
      expect(labels()).not.toContain('删除 user-1 的回复')
    })

    it('cancelling a confirmation sends nothing', async () => {
      await withReplies([reply(2, 'user-1', 'keep me')])
      root.querySelector<HTMLButtonElement>('button[aria-label="删除 user-1 的回复"]')!.click(); await flush()
      click('取消'); await flush()
      expect(mocks.command).not.toHaveBeenCalled(); expect(root.textContent).toContain('keep me')
    })

    it('edits the issue title, category and description with only the changed fields', async () => {
      root.querySelector<HTMLButtonElement>('button[aria-label="编辑主题"]')!.click(); await flush()
      const form = root.querySelector('form[aria-label="编辑主题"]')!
      fill(form.querySelector<HTMLInputElement>('input[aria-label="主题标题"]')!, 'Renamed'); await flush()
      mocks.command.mockResolvedValue({ issue: { ...issue, title: 'Renamed', version: 4, activity_seq: 9, edited_at: 1700000200 }, replayed: false })
      mocks.detail.mockResolvedValue({ issue: { ...issue, title: 'Renamed', version: 4, activity_seq: 9, edited_at: 1700000200 }, events: [], next_event_cursor: 9, has_more: false })
      click('保存'); await flush()
      const sent = mocks.command.mock.calls[0]![2]
      expect(sent).toMatchObject({ action: 'edit_issue', title: 'Renamed', expected_version: 3 })
      expect(sent).not.toHaveProperty('description'); expect(sent).not.toHaveProperty('category')
      expect(root.querySelector('h1')?.textContent).toContain('Renamed')
    })

    it('deleting the issue asks first, then leaves the page and reloads the list', async () => {
      click('删除主题'); await flush()
      expect(mocks.command).not.toHaveBeenCalled()
      expect(root.querySelector('[role="alertdialog"]')?.textContent).toContain('编号不会被复用')
      mocks.command.mockResolvedValue({ issue: { ...issue, deleted_at: 1700000300, version: 4 }, replayed: false })
      mocks.detail.mockClear(); mocks.list.mockClear()
      click('确认删除'); await flush()
      expect(mocks.command).toHaveBeenCalledWith(false, 'issue-id', expect.objectContaining({ action: 'delete_issue', expected_version: 3 }))
      expect(mocks.push).toHaveBeenCalledWith('/dashboard/feedback')
      expect(mocks.list).toHaveBeenCalled()
      expect(root.textContent).toContain('主题已删除')
    })

    it('shows edits by others as a system line and hides the bookkeeping markers', async () => {
      await withReplies([
        { ...reply(2, 'root', 'x'), kind: 'issue_edit', content: 'title,description', actor_role: 'admin' },
        { ...reply(3, 'root', null), kind: 'reply_edit', target_seq: 2 },
        { ...reply(4, 'root', null), kind: 'reply_delete', target_seq: 2 },
      ])
      expect(root.textContent).toContain('编辑了主题（标题、描述）')
      expect(root.querySelectorAll('.timeline-entry').length).toBe(2) // opening post + the edit line
    })
  })
  it('creates public topics only: no visibility control and none is sent', async () => {
    click('发起主题'); await flush()
    const dialog = root.querySelector('[data-dialog="发起主题"]')!
    expect(dialog.querySelector('[aria-label="反馈可见范围"]')).toBeNull()
    expect(dialog.textContent).toContain('对所有已登录用户公开')
    fill(dialog.querySelector<HTMLInputElement>('input')!, 'Share issue'); fill(dialog.querySelector<HTMLTextAreaElement>('textarea')!, 'Safe description'); await flush(); click('发起'); await flush()
    expect(mocks.create).toHaveBeenCalledWith(expect.objectContaining({ title: 'Share issue', include_diagnostics: false }), false)
    expect(mocks.create.mock.lastCall![0]).not.toHaveProperty('visibility')
  })
  it('opens focus mode from the new-topic form, keeps the draft, and can submit from it', async () => {
    click('发起主题'); await flush()
    const dialog = root.querySelector('[data-dialog="发起主题"]')!
    fill(dialog.querySelector<HTMLInputElement>('input')!, 'Long topic'); fill(dialog.querySelector<HTMLTextAreaElement>('textarea')!, 'Draft body'); await flush()
    expect(document.body.querySelector('.focus-layer')).toBeNull()
    ;(dialog.querySelector('.focus-toggle') as HTMLButtonElement).click(); await flush()
    const layer = document.body.querySelector('.focus-layer')!
    expect(layer).toBeTruthy()
    // The form's draft is what the focus layer shows.
    expect(layer.querySelector<HTMLInputElement>('input.focus-title')!.value).toBe('Long topic')
    expect(layer.querySelector<HTMLTextAreaElement>('textarea')!.value).toBe('Draft body')
    fill(layer.querySelector<HTMLTextAreaElement>('textarea')!, 'Draft body, extended in focus mode'); await flush()
    // Leaving keeps what was typed.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true })); await flush()
    expect(document.body.querySelector('.focus-layer')).toBeNull()
    expect(dialog.querySelector<HTMLTextAreaElement>('textarea')!.value).toBe('Draft body, extended in focus mode')
    // Submitting from the focus layer creates the topic and closes both.
    ;(dialog.querySelector('.focus-toggle') as HTMLButtonElement).click(); await flush()
    ;([...document.body.querySelectorAll('.focus-layer button')].find(b => b.textContent?.includes('发起主题')) as HTMLButtonElement).click(); await flush()
    expect(mocks.create).toHaveBeenCalledWith(expect.objectContaining({ title: 'Long topic', description: 'Draft body, extended in focus mode' }), false)
    expect(document.body.querySelector('.focus-layer')).toBeNull()
  })
  it('marks topics from before private ones were removed, and nothing else, as private', async () => {
    mocks.detail.mockResolvedValue({ issue: { ...issue, visibility: 'private' }, events: [{ seq: 8, kind: 'reply', actor_id: 'user-9', actor_username: 'Old', actor_role: 'user', visibility: 'private', content: 'Old secret', created_at: 1700000000 }], has_more: false, next_event_cursor: null })
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    expect(root.textContent).toContain('历史私密主题')
    expect(root.textContent).toContain('历史私密')
    mocks.detail.mockResolvedValue({ issue: { ...issue, visibility: 'public' }, events: [], has_more: false, next_event_cursor: null })
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    expect(root.textContent).not.toContain('历史私密')
  })
  it('replying to someone prefills an @mention and focuses the editor', async () => {
    mocks.detail.mockResolvedValue({ issue, events: [{ seq: 8, kind: 'reply', actor_id: 'user-2', actor_username: 'Participant', actor_role: 'user', visibility: 'public', content: 'A public detail', created_at: 1700000000 }], has_more: false, next_event_cursor: null })
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    root.querySelector<HTMLButtonElement>('button[aria-label="回复 Participant"]')!.click(); await flush()
    expect(root.querySelector<HTMLTextAreaElement>('#feedback-reply')!.value).toBe('@Participant ')
    expect(document.activeElement?.id).toBe('feedback-reply')
  })
  it('separates the issue list from the discussion and preserves back navigation', async () => {
    expect(root.querySelector('[aria-label="主题列表"]')).toBeNull()
    click('所有主题'); await flush()
    expect(root.querySelector('[aria-label="主题列表"]')).toBeTruthy(); expect(root.querySelector('#feedback-reply')).toBeNull()
    expect(root.querySelector('[aria-label="主题状态"]')).toBeTruthy()
  })
  it('shows the issue number and lets only administrators pin and unpin', async () => {
    mocks.detail.mockResolvedValue({ issue: { ...issue, number: 12 }, events: [], next_event_cursor: null, has_more: false }); mocks.route.params = { issueId: '12' }; await flush()
    expect(root.querySelector('.issue-heading h1')?.textContent).toContain('#12')
    expect([...root.querySelectorAll('button')].some(b => b.textContent?.includes('置顶'))).toBe(false)
    mocks.auth.user = { id: 'root', role: 'admin' }; mocks.route.path = '/admin/feedback/12'; await flush()
    const pinnedIssue = { ...issue, number: 12, pinned_at: 5, version: 4, activity_seq: 8 }
    mocks.command.mockResolvedValue({ issue: pinnedIssue, replayed: false })
    mocks.detail.mockResolvedValue({ issue: pinnedIssue, events: [], next_event_cursor: null, has_more: false })
    click('置顶'); await flush()
    expect(mocks.command).toHaveBeenLastCalledWith(true, '12', expect.objectContaining({ action: 'set_pinned', pinned: true, expected_version: 3 }))
    expect(root.textContent).toContain('已置顶')
    click('取消置顶'); await flush()
    expect(mocks.command).toHaveBeenLastCalledWith(true, '12', expect.objectContaining({ action: 'set_pinned', pinned: false, expected_version: 4 }))
  })
  it('suggests users after @ and inserts the chosen name', async () => {
    const textarea = root.querySelector<HTMLTextAreaElement>('#feedback-reply')!
    textarea.value = 'cc @al'; textarea.setSelectionRange(6, 6); textarea.dispatchEvent(new Event('input', { bubbles: true })); await flush()
    const options = [...root.querySelectorAll('.mention-list button')]; expect(options.map(o => o.textContent?.trim())).toEqual(['@Alice'])
    options[0]!.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true })); await flush()
    expect(textarea.value).toBe('cc @Alice ')
  })
  it('never suggests yourself after @, while still offering everybody else', async () => {
    // The suggestion list of this suite is ['root', 'Alice']; the signed-in user is root.
    mocks.auth.user = { id: 'user-1', role: 'user', username: 'ROOT' }
    await flush()
    const textarea = root.querySelector<HTMLTextAreaElement>('#feedback-reply')!
    textarea.value = '@'; textarea.setSelectionRange(1, 1); textarea.dispatchEvent(new Event('input', { bubbles: true })); await flush()
    expect([...root.querySelectorAll('.mention-list button')].map(o => o.textContent?.trim())).toEqual(['@Alice'])
    textarea.value = '@ro'; textarea.setSelectionRange(3, 3); textarea.dispatchEvent(new Event('input', { bubbles: true })); await flush()
    expect(root.querySelectorAll('.mention-list button')).toHaveLength(0)
    // Somebody else's name is still found.
    textarea.value = '@al'; textarea.setSelectionRange(3, 3); textarea.dispatchEvent(new Event('input', { bubbles: true })); await flush()
    expect([...root.querySelectorAll('.mention-list button')].map(o => o.textContent?.trim())).toEqual(['@Alice'])
  })
  it('shows pictures for the people who have one and initials for the rest', async () => {
    mocks.detail.mockResolvedValue({
      issue,
      events: [
        { seq: 8, kind: 'reply', actor_id: 'user-2', actor_username: 'Participant', actor_role: 'user', visibility: 'public', content: 'hello', created_at: 1700000100 },
        { seq: 9, kind: 'reply', actor_id: 'user-3', actor_username: 'Quiet', actor_role: 'user', visibility: 'public', content: 'hi', created_at: 1700000200 },
      ],
      avatars: { 'user-1': 111, 'user-2': 222 },
      has_more: false,
      next_event_cursor: 9,
    })
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    const pictures = [...root.querySelectorAll('.timeline-entry .user-avatar')].map(node => node.querySelector('img')?.getAttribute('src') ?? node.textContent?.trim())
    expect(pictures).toEqual(['/api/avatars/user-1?v=111', '/api/avatars/user-2?v=222', 'Q'])
  })
  it('previews markdown with images, issue references and mentions', async () => {
    const textarea = root.querySelector<HTMLTextAreaElement>('#feedback-reply')!
    fill(textarea, '**bold** #7 @root ![shot](https://example.com/a.png)'); await flush()
    click('预览'); await flush()
    const preview = root.querySelector('.editor-preview')!
    expect(preview.querySelector('strong')?.textContent).toBe('bold')
    expect(preview.querySelector('a.fb-ref')?.getAttribute('href')).toBe('/dashboard/feedback/7')
    expect(preview.querySelector('.fb-mention')?.textContent).toBe('@root')
    expect(preview.querySelector('img')?.getAttribute('src')).toBe('https://example.com/a.png')
  })
})
