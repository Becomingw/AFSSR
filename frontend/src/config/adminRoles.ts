export type AdminRole = 'admin' | 'audit_admin' | 'user_admin' | 'service_admin'

export function canAccessAdminRole(role?: string): boolean {
  return ['admin', 'audit_admin', 'user_admin', 'service_admin'].includes(role ?? '')
}
export function canManageUsersRole(role?: string): boolean {
  return role === 'admin' || role === 'user_admin'
}
export function canManageServicesRole(role?: string): boolean {
  return role === 'admin' || role === 'service_admin'
}
const overview = ['/admin/dashboard', '/admin/operations', '/admin/health-monitor', '/admin/user-stats', '/admin/cost-analysis', '/admin/performance-analysis', '/admin/usage']
const users = ['/admin/users', '/admin/access-policies', '/admin/announcements', '/admin/keys']
const services = ['/admin/providers', '/admin/models', '/admin/routing', '/admin/pool', '/admin/async-tasks', '/admin/proxy-nodes', '/admin/gemini-files']
export function canAccessAdminPage(role: string | undefined, path: string): boolean {
  if (!canAccessAdminRole(role)) return false
  if (role === 'admin' || role === 'audit_admin') return true
  if (path === '/admin/mcp') return true
  if (path === '/admin/compatibility') return true
  if (path === '/admin/feedback' || path.startsWith('/admin/feedback/')) return true
  if (overview.includes(path) || path === '/admin/leaderboard') return true
  const pages = role === 'user_admin' ? users : services
  return pages.some(page => path === page || path.startsWith(`${page}/`))
}
export function adminRoleLabel(role?: string): string {
  switch (role) {
    case 'admin': return '超级管理员'
    case 'audit_admin': return '审计管理员'
    case 'user_admin': return '用户及权益管理员'
    case 'service_admin': return '服务管理员'
    default: return '普通用户'
  }
}
