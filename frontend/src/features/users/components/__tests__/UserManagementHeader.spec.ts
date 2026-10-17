import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, h, nextTick, ref, type App } from 'vue'
import UserManagementHeader from '../UserManagementHeader.vue'
import UserActionButtons from '../UserActionButtons.vue'
import UserManagementList from '../UserManagementList.vue'

vi.mock('@/i18n', async importOriginal => ({
  ...await importOriginal<typeof import('@/i18n')>(),
  useI18n: () => ({ legacyT: (text: string) => text }),
}))

let app: App | undefined
let root: HTMLDivElement
afterEach(() => { app?.unmount(); root?.remove() })

async function header(canOperateAdmin: boolean) {
  const create = vi.fn()
  const refresh = vi.fn()
  const search = vi.fn()
  root = document.createElement('div')
  document.body.append(root)
  app = createApp(UserManagementHeader, {
    searchQuery: '', filterRole: 'all', filterGroup: 'all', filterStatus: 'all',
    sortOption: 'default', userGroups: [],
    roleOptions: [{ value: 'all', label: '全部角色' }, { value: 'user_admin', label: '用户及权益管理员' }],
    statusOptions: [{ value: 'all', label: '全部状态' }],
    sortOptions: [{ value: 'default', label: '默认排序' }],
    loading: false, canOperateAdmin,
    onCreateUser: create, onRefresh: refresh, 'onUpdate:searchQuery': search,
  })
  app.mount(root)
  await nextTick()
  return { create, refresh, search }
}

describe('user console presentation', () => {
  it('distinguishes an initial load from an empty result instead of flashing no users', async () => {
    root = document.createElement('div')
    document.body.append(root)
    const loading = ref(true)
    app = createApp({ setup: () => () => h(UserManagementList, {
      rows: [], selectedIdSet: new Set<string>(), selectAllFiltered: false,
      isAllFilteredSelected: false, isPartiallyFilteredSelected: false,
      isCurrentPageFullySelected: false, selectionDisabled: false,
      loading: loading.value, canOperateAdmin: false, hasFilters: false,
      sortBy: null, sortOrder: 'desc',
    }) })
    app.mount(root)
    expect(root.querySelector('[aria-busy="true"]')).not.toBeNull()
    expect(root.textContent).toContain('加载中...')
    expect(root.textContent).not.toContain('暂无用户')
    loading.value = false
    await nextTick()
    expect(root.querySelector('[aria-busy="false"]')).not.toBeNull()
    expect(root.textContent).toContain('暂无用户')
    expect(root.textContent).not.toContain('加载中...')
  })
  it('has one page heading and one accessible search/filter set at every breakpoint', async () => {
    const events = await header(true)
    expect(root.querySelectorAll('h1')).toHaveLength(1)
    expect(root.querySelectorAll('input[type="text"]')).toHaveLength(1)
    for (const label of ['角色', '状态', '分组', '排序']) {
      expect(root.querySelectorAll(`[role="combobox"][aria-label="${label}"]`)).toHaveLength(1)
    }
    const input = root.querySelector<HTMLInputElement>('input[aria-label="搜索用户名或邮箱"]')!
    input.value = 'research'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    root.querySelector<HTMLButtonElement>('button[title="新增用户"]')!.click()
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click()
    expect(events.search).toHaveBeenCalledWith('research')
    expect(events.create).toHaveBeenCalledOnce()
    expect(events.refresh).toHaveBeenCalledOnce()
  })

  it('keeps audit controls read-only without hiding filters and refresh', async () => {
    const events = await header(false)
    expect(root.querySelector('button[title="新增用户"]')).toBeNull()
    expect(root.querySelector('button[title="分组管理"]')).toBeNull()
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click()
    expect(events.refresh).toHaveBeenCalledOnce()
    expect(root.querySelector('input')).not.toBeNull()
  })

  it('keeps compact icon actions labelled and preserves edit events', async () => {
    root = document.createElement('div')
    document.body.append(root)
    const edit = vi.fn()
    app = createApp(UserActionButtons, { canOperateAdmin: true, isActive: true, mobile: true, onEdit: edit })
    app.mount(root)
    await nextTick()
    const editButton = root.querySelector<HTMLButtonElement>('button[title="编辑用户"]')!
    expect(editButton.textContent).toContain('编辑')
    expect(editButton.querySelector('.sr-only')).not.toBeNull()
    editButton.click()
    expect(edit).toHaveBeenCalledOnce()
  })
})
