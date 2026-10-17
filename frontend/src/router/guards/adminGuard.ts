import type { RouteLocationNormalized } from 'vue-router'
import type { useAuthStore } from '@/stores/auth'
import type { useModuleStore } from '@/stores/modules'
import { log } from '@/utils/logger'
import { canAccessAdminPage } from '@/config/adminRoles'

/**
 * 检查管理员权限和管理端模块可用性。
 * @returns 重定向路径，或 null 表示通过
 */
export async function checkAdminAccess(
  to: RouteLocationNormalized,
  authStore: ReturnType<typeof useAuthStore>,
  moduleStore: ReturnType<typeof useModuleStore>
): Promise<string | null> {
  if (!authStore.canAccessAdmin) {
    log.warn('Non-admin user attempted to access admin page, redirecting to user dashboard')
    return '/dashboard'
  }

  if (!canAccessAdminPage(authStore.user?.role, to.path)) return '/admin/dashboard'

  // 检查路由链中是否有模块要求
  const moduleName = to.matched.find(record => record.meta.module)?.meta.module as
    | string
    | undefined
  if (!moduleName) {
    return null
  }

  // 确保模块状态已加载
  if (!moduleStore.loaded) {
    try {
      await moduleStore.fetchModules()
    } catch (error) {
      // fail-close: 获取模块状态失败时拒绝访问
      log.warn('Failed to fetch modules status, denying access', { error })
      return '/admin/dashboard'
    }
  }

  // 如果模块不可用（未部署），重定向到管理员首页
  // 注意：只检查 available，不检查 enabled/active，允许管理员配置未启用的模块
  if (!moduleStore.isAvailable(moduleName)) {
    log.warn(`Module ${moduleName} is not available, redirecting to admin dashboard`)
    return '/admin/dashboard'
  }

  return null
}
