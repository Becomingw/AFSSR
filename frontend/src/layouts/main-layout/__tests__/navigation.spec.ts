import { describe, expect, it } from 'vitest'
import type { RouteLocationNormalizedLoaded } from 'vue-router'

import { buildBreadcrumbs, buildNavigation } from '@/layouts/main-layout/navigation'
import { dashboardRoutes } from '@/router/routes/dashboard'
import type { MessageKey } from '@/i18n'

const translate = (key: MessageKey) => `tx:${key}`

function route(path: string, name?: string, meta: Record<string, unknown> = {}): RouteLocationNormalizedLoaded {
  return {
    path,
    fullPath: path,
    query: {},
    hash: '',
    name,
    params: {},
    matched: [],
    meta,
    redirectedFrom: undefined,
  } as RouteLocationNormalizedLoaded
}

describe('main layout navigation builder', () => {
  it('redirects retired remote-control bookmarks to client API keys', () => {
    const legacy = dashboardRoutes[0]?.children?.find(route => route.path === 'vscodex')
    expect(legacy?.redirect).toBe('/dashboard/api-keys')
    expect(legacy?.component).toBeUndefined()
  })
  it('filters menus to the delegated administrator domain', () => {
    const menu = (role: string) => buildNavigation({ canAccessAdmin: true, role, modules: {}, isModuleActive: () => false }).flatMap(group => group.items.map(item => item.href))
    expect(menu('user_admin')).toContain('/admin/access-policies')
    expect(menu('user_admin')).not.toContain('/admin/providers')
    expect(menu('service_admin')).toContain('/admin/pool')
    expect(menu('service_admin')).not.toContain('/admin/users')
    expect(menu('service_admin')).not.toContain('/admin/system')
  })
  it('builds user navigation from translation keys and active modules', () => {
    const navigation = buildNavigation({
      canAccessAdmin: false,
      modules: {},
      isModuleActive: (name) => name === 'referral',
      t: translate,
    })

    expect(navigation.map(group => group.title)).toEqual([
      'tx:nav.group.overview',
      'tx:nav.group.resources',
      'tx:nav.group.account',
    ])
    const links = navigation.flatMap(group => group.items.map(item => item.href))
    expect(links).not.toContain('/dashboard/referral')
    expect(links).not.toContain('/dashboard/wallet')
    expect(links).not.toContain('/dashboard/billing')
  })

  it('does not expose retired remote control even with stale module metadata', () => {
    const userNavigation = buildNavigation({
      canAccessAdmin: false,
      modules: {},
      isModuleActive: () => false,
      t: translate,
    })
    const adminNavigation = buildNavigation({
      canAccessAdmin: true,
      modules: {
        vscodex: {
          active: true,
          name: 'test-module',
          available: true,
          enabled: true,
          config_validated: true,
          config_error: null,
          description: '',
          category: 'integration',
          health: 'healthy',
          admin_route: '/dashboard/vscodex',
          admin_menu_group: 'overview',
          admin_menu_order: 80,
          admin_menu_icon: 'SquareTerminal',
          display_name: '远程控制',
        },
      },
      isModuleActive: () => false,
      t: translate,
    })

    const findVscodeControl = (navigation: ReturnType<typeof buildNavigation>) => (
      navigation
        .flatMap(group => group.items)
        .find(item => item.href === '/dashboard/vscodex')
    )

    expect(findVscodeControl(userNavigation)).toBeUndefined()
    expect(findVscodeControl(adminNavigation)).toBeUndefined()

    const overviewItems = adminNavigation.find(group => group.title === 'tx:nav.group.overview')?.items ?? []
    expect(overviewItems.some(item => item.name === '远程控制')).toBe(false)
  })

  it('builds admin navigation with dynamic module menu items sorted by menu order', () => {
    const navigation = buildNavigation({
      canAccessAdmin: true,
      modules: {
        first: {
          active: true,
          name: 'test-module',
          available: true,
          enabled: true,
          config_validated: true,
          config_error: null,
          description: '',
          category: 'integration',
          health: 'healthy',
          admin_route: '/admin/first',
          admin_menu_group: 'management',
          admin_menu_order: 2,
          admin_menu_icon: 'Gift',
          display_name: 'First module',
        },
        second: {
          active: true,
          name: 'test-module',
          available: true,
          enabled: true,
          config_validated: true,
          config_error: null,
          description: '',
          category: 'integration',
          health: 'healthy',
          admin_route: '/admin/second',
          admin_menu_group: 'management',
          admin_menu_order: 1,
          admin_menu_icon: 'Key',
          display_name: 'Second module',
        },
      },
      isModuleActive: () => false,
      t: translate,
    })

    const managementItems = navigation.find(group => group.title === 'tx:nav.group.management')?.items ?? []
    expect(managementItems.map(item => item.name)).toEqual(expect.arrayContaining(['Second module', 'First module']))
    expect(managementItems.findIndex(item => item.name === 'Second module')).toBeLessThan(
      managementItems.findIndex(item => item.name === 'First module')
    )
  })

  it('builds translated breadcrumbs for settings and routing detail pages', () => {
    const navigation = buildNavigation({
      canAccessAdmin: true,
      modules: {},
      isModuleActive: () => false,
      t: translate,
    })

    expect(buildBreadcrumbs({
      route: route('/dashboard/settings'),
      navigation,
      modules: {},
      isNavActive: () => false,
      t: translate,
    })).toEqual([
      { label: 'tx:nav.group.account' },
      { label: 'tx:breadcrumb.personalSettings' },
    ])

    expect(buildBreadcrumbs({
      route: route('/admin/routing/new', 'RoutingProfileCreate'),
      navigation,
      modules: {},
      isNavActive: href => href === '/admin/routing',
      t: translate,
    })).toEqual([
      { label: 'tx:nav.group.management' },
      { label: 'tx:nav.routing', href: '/admin/routing' },
      { label: 'tx:breadcrumb.routingCreate' },
    ])

  })
})
