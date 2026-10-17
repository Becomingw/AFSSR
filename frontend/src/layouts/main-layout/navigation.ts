import type { RouteLocationNormalizedLoaded } from 'vue-router'
import type { LucideIcon } from 'lucide-vue-next'
import {
  Activity,
  Trophy,
  MessagesSquare,
  Bot,
  BarChart3,
  Box,
  Cog,
  CreditCard,
  Database,
  FileUp,
  FolderTree,
  Gift,
  Gauge,
  Home,
  Key,
  KeyRound,
  Layers,
  Puzzle,
  Send,
  Server,
  Shield,
  SlidersHorizontal,
  SquareTerminal,
  Users,
  UserRound,
  Zap,
  Megaphone,
} from 'lucide-vue-next'
import type { NavigationGroup } from '@/components/layout/SidebarNav.vue'
import type { ModuleStatus } from '@/api/modules'
import { BUILTIN_TOOL_BREADCRUMBS } from '@/config/builtin-tools'
import type { MessageKey } from '@/i18n'
import { COMMERCIAL_MODULES } from '@/config/internalGateway'
import { canAccessAdminPage } from '@/config/adminRoles'

type ModuleRecord = Record<string, ModuleStatus>
type Translate = (key: MessageKey) => string

export interface BreadcrumbItem {
  label: string
  href?: string
}

type NavItem = NavigationGroup['items'][number]

const moduleIconMap: Record<string, LucideIcon> = {
  Key,
  KeyRound,
  FileUp,
  Shield,
  Puzzle,
  Server,
  Send,
  SlidersHorizontal,
  SquareTerminal,
  CreditCard,
  Gift,
}

function activeModuleItems(modules: ModuleRecord, group: string): NavItem[] {
  return Object.values(modules)
    .filter(m => m.active && m.admin_route && m.admin_route !== '/dashboard/vscodex' && m.name !== 'vscodex' && m.admin_menu_group === group && !COMMERCIAL_MODULES.has(m.name))
    .sort((a, b) => a.admin_menu_order - b.admin_menu_order)
    .map(m => ({
      name: m.display_name,
      href: m.admin_route ?? '',
      icon: moduleIconMap[m.admin_menu_icon || ''] || Puzzle
    }))
}

export function buildNavigation(options: {
  canAccessAdmin: boolean
  role?: string
  modules: ModuleRecord
  isModuleActive: (name: string) => boolean
  t?: Translate
}): NavigationGroup[] {
  const { canAccessAdmin, modules } = options
  const t = options.t ?? ((key: MessageKey) => key)

  if (!canAccessAdmin) {
    return [
      {
        title: t('nav.group.overview'),
        items: [
          { name: t('nav.dashboard'), href: '/dashboard', icon: Home },
          { name: '使用排行榜', href: '/dashboard/leaderboard', icon: Trophy },
          { name: t('nav.healthMonitor'), href: '/dashboard/endpoint-status', icon: Activity },
        ]
      },
      {
        title: t('nav.group.resources'),
        items: [
          { name: t('nav.modelCatalog'), href: '/dashboard/models', icon: Box },
          { name: t('nav.apiKeys'), href: '/dashboard/api-keys', icon: Key },
        ]
      },
      {
        title: t('nav.group.account'),
        items: [
          { name: '个人资料', href: '/dashboard/settings', icon: UserRound },
          { name: '讨论与反馈', href: '/dashboard/feedback', icon: MessagesSquare },
          { name: t('nav.usageStats'), href: '/dashboard/usage', icon: BarChart3 },
        ]
      }
    ]
  }

  const systemItems: NavItem[] = [
    { name: '讨论与反馈', href: '/admin/feedback', icon: MessagesSquare },
    { name: 'Agent MCP', href: '/admin/mcp', icon: Bot },
    { name: '模型兼容性', href: '/admin/compatibility', icon: SlidersHorizontal },
    { name: t('nav.announcements'), href: '/admin/announcements', icon: Megaphone },
    { name: t('nav.cacheMonitoring'), href: '/admin/cache-monitoring', icon: Gauge },
    ...activeModuleItems(modules, 'system'),
    { name: t('nav.moduleManagement'), href: '/admin/modules', icon: Puzzle },
    { name: t('nav.systemSettings'), href: '/admin/system', icon: Cog },
  ]

  const groups: NavigationGroup[] = [
    {
      title: t('nav.group.overview'),
      items: [
        { name: t('nav.dashboard'), href: '/admin/dashboard', icon: Home },
        { name: t('nav.operations'), href: '/admin/operations', icon: Activity },
        { name: t('nav.healthMonitor'), href: '/admin/health-monitor', icon: Activity },
        { name: t('nav.userStats'), href: '/admin/user-stats', icon: BarChart3 },
        { name: '使用排行榜', href: '/admin/leaderboard', icon: Trophy },
        { name: t('nav.costAnalysis'), href: '/admin/cost-analysis', icon: Gauge },
        { name: t('nav.performanceAnalysis'), href: '/admin/performance-analysis', icon: Activity },
        ...activeModuleItems(modules, 'overview'),
      ]
    },
    {
      title: t('nav.group.management'),
      items: [
        { name: t('nav.userManagement'), href: '/admin/users', icon: Users },
        { name: '用户权益', href: '/admin/access-policies', icon: Shield },
        { name: t('nav.providers'), href: '/admin/providers', icon: FolderTree },
        { name: t('nav.modelManagement'), href: '/admin/models', icon: Layers },
        { name: t('nav.routing'), href: '/admin/routing', icon: SlidersHorizontal },
        { name: t('nav.pool'), href: '/admin/pool', icon: Database },
        { name: t('nav.standaloneKeys'), href: '/admin/keys', icon: Key },
        ...activeModuleItems(modules, 'management'),
        { name: t('nav.asyncTasks'), href: '/admin/async-tasks', icon: Zap },
        { name: t('nav.usageRecords'), href: '/admin/usage', icon: BarChart3 },
      ]
    },
    {
      title: t('nav.group.system'),
      items: systemItems
    }
  ]
  return groups.map(group => ({ ...group, items: group.items.filter(item => canAccessAdminPage(options.role ?? 'admin', item.href)) })).filter(group => group.items.length > 0)
}

export function buildBreadcrumbs(options: {
  route: RouteLocationNormalizedLoaded
  navigation: NavigationGroup[]
  modules: ModuleRecord
  isNavActive: (href: string) => boolean
  t?: Translate
}): BreadcrumbItem[] {
  const { route, navigation, modules, isNavActive } = options
  const t = options.t ?? ((key: MessageKey) => key)

  if (route.path === '/dashboard/settings') {
    return [
      { label: t('nav.group.account') },
      { label: t('breadcrumb.personalSettings') }
    ]
  }

  if (route.meta?.module) {
    const moduleName = route.meta.module as string
    const moduleStatus = modules[moduleName]
    const displayName = moduleStatus?.display_name || moduleName
    return [
      { label: t('nav.group.system') },
      { label: t('nav.moduleManagement'), href: '/admin/modules' },
      { label: displayName }
    ]
  }

  if (BUILTIN_TOOL_BREADCRUMBS[route.path]) {
    return [
      { label: t('nav.group.system') },
      { label: t('nav.moduleManagement'), href: '/admin/modules' },
      { label: BUILTIN_TOOL_BREADCRUMBS[route.path] }
    ]
  }

  if (route.path.startsWith('/admin/routing/') && route.path !== '/admin/routing') {
    return [
      { label: t('nav.group.management') },
      { label: t('nav.routing'), href: '/admin/routing' },
      {
        label: route.name === 'RoutingProfileCreate'
          ? t('breadcrumb.routingCreate')
          : t('breadcrumb.routingConfig')
      }
    ]
  }

  for (const group of navigation) {
    const activeItem = group.items.find(item => isNavActive(item.href))
    if (activeItem) {
      return [
        { label: group.title || '' },
        { label: activeItem.name }
      ]
    }
  }

  const currentModule = Object.values(modules).find(
    m => m.admin_route && route.path === m.admin_route
  )
  if (currentModule) {
    return [
      { label: t('nav.moduleManagement'), href: '/admin/modules' },
      { label: currentModule.display_name }
    ]
  }

  return [{ label: t('nav.dashboard') }]
}
