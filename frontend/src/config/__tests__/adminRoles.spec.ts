import { describe, expect, it } from 'vitest'
import { adminRoleLabel, canAccessAdminPage, canAccessAdminRole, canManageServicesRole, canManageUsersRole } from '../adminRoles'

describe('administrator role boundaries', () => {
  it('keeps global administration exclusive to the original admin role', () => {
    expect(adminRoleLabel('admin')).toBe('超级管理员')
    expect(canAccessAdminPage('admin', '/admin/system')).toBe(true)
    for (const role of ['user_admin', 'service_admin']) expect(canAccessAdminPage(role, '/admin/system')).toBe(false)
    expect(canAccessAdminRole('user')).toBe(false)
    expect(canAccessAdminRole('unknown')).toBe(false)
  })
  it('gives user and benefits administrators user workflows, not backend channel configuration', () => {
    expect(canManageUsersRole('user_admin')).toBe(true)
    expect(canManageServicesRole('user_admin')).toBe(false)
    expect(canAccessAdminPage('user_admin', '/admin/access-policies')).toBe(true)
    expect(canAccessAdminPage('user_admin', '/admin/users')).toBe(true)
    expect(canAccessAdminPage('user_admin', '/admin/pool')).toBe(false)
    expect(canAccessAdminPage('user_admin', '/admin/providers')).toBe(false)
  })
  it('gives service administrators service workflows, not user access management', () => {
    expect(canManageServicesRole('service_admin')).toBe(true)
    expect(canManageUsersRole('service_admin')).toBe(false)
    expect(canAccessAdminPage('service_admin', '/admin/providers')).toBe(true)
    expect(canAccessAdminPage('service_admin', '/admin/routing/group-1')).toBe(true)
    expect(canAccessAdminPage('service_admin', '/admin/users')).toBe(false)
    expect(canAccessAdminPage('service_admin', '/admin/access-policies')).toBe(false)
  })
  it('preserves audit access without granting either write domain', () => {
    expect(canAccessAdminPage('audit_admin', '/admin/usage')).toBe(true)
    expect(canManageUsersRole('audit_admin')).toBe(false)
    expect(canManageServicesRole('audit_admin')).toBe(false)
  })
})
