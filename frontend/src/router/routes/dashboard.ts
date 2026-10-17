import type { RouteRecordRaw } from 'vue-router'
import { view } from './helpers'

export const dashboardRoutes: RouteRecordRaw[] = [
  {
    path: '/dashboard',
    component: view(() => import('@/layouts/MainLayout.vue')),
    meta: { requiresAuth: true },
    children: [
      { path: 'leaderboard', name: 'UserLeaderboard', component: view(() => import('@/views/shared/Leaderboard.vue')) },
      { path: 'feedback/:issueId?', name: 'MyFeedback', component: view(() => import('@/views/shared/Feedback.vue')) },
      {
        path: '',
        name: 'Dashboard',
        component: view(() => import('@/views/shared/Dashboard.vue'))
      },
      {
        path: 'api-keys',
        name: 'MyApiKeys',
        component: view(() => import('@/views/user/MyApiKeys.vue'))
      },
      {
        path: 'management-tokens',
        name: 'ManagementTokens',
        component: view(() => import('@/views/user/ManagementTokens.vue')),
        meta: { module: 'management_tokens' }
      },
      {
        path: 'announcements',
        name: 'Announcements',
        component: view(() => import('@/views/user/Announcements.vue'))
      },
      {
        path: 'usage',
        name: 'MyUsage',
        component: view(() => import('@/views/shared/Usage.vue'))
      },
      {
        path: 'endpoint-status',
        name: 'EndpointStatus',
        component: view(() => import('@/views/shared/HealthMonitor.vue'))
      },
      {
        path: 'settings',
        alias: 'profile',
        name: 'Settings',
        component: view(() => import('@/views/user/Profile.vue'))
      },
      {
        path: 'models',
        name: 'ModelCatalog',
        component: view(() => import('@/views/user/ModelCatalog.vue'))
      },
      {
        path: 'vscodex',
        redirect: '/dashboard/api-keys'
      },
      {
        path: 'async-tasks',
        name: 'UserAsyncTasks',
        component: view(() => import('@/views/admin/AsyncTasks.vue'))
      }
    ]
  }
]
