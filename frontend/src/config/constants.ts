/**
 * 应用全局常量配置
 */

// 应用配置
export const APP_CONFIG = {
  NAME: 'Arca',
  VERSION: '9.1.0',
  DESCRIPTION: 'AI 模型接入与权益管理平台',
} as const

// 网络配置
export const NETWORK_CONFIG = {
  API_TIMEOUT: 30000, // 30秒
  REQUEST_RETRY_LIMIT: 3, // 最大重试次数
  MODULE_LOAD_RETRY_LIMIT: 2, // 模块加载失败重试次数
} as const

// 认证配置
export const AUTH_CONFIG = {
  TOKEN_REFRESH_INTERVAL: 100, // 延迟检查认证状态(ms)
  MAX_RETRY_COUNT: 3, // token刷新最大重试次数
} as const

// Toast 配置
export const TOAST_CONFIG = {
  SUCCESS_DURATION: 3000, // 成功消息持续时间(ms)
  ERROR_DURATION: 5000, // 错误消息持续时间(ms)
  WARNING_DURATION: 5000, // 警告消息持续时间(ms)
  INFO_DURATION: 3000, // 信息消息持续时间(ms)
} as const

// 日志级别
export const LogLevel = {
  DEBUG: 'DEBUG',
  INFO: 'INFO',
  WARN: 'WARN',
  ERROR: 'ERROR',
} as const

export type LogLevel = typeof LogLevel[keyof typeof LogLevel]

// 业务相关常量
export const BUSINESS_CONSTANTS = {
  // Provider 相关
  PROVIDER_DEFAULT_PRIORITY: 100,
  PROVIDER_MIN_PRIORITY: 0,
  PROVIDER_MAX_PRIORITY: 999,

  // API Key 相关（不限制长度）

  // 分页配置
  DEFAULT_PAGE_SIZE: 20,
  MAX_PAGE_SIZE: 100,
} as const

// 错误消息
export const ERROR_MESSAGES = {
  NETWORK_ERROR: '无法连接到服务器,请检查网络连接',
  AUTH_FAILED: '认证失败,请重新登录',
  PERMISSION_DENIED: '权限不足',
  SERVER_ERROR: '服务器错误,请稍后重试',
  INVALID_INPUT: '输入数据无效',
  OPERATION_FAILED: '操作失败',
} as const

// 环境判断
export const isDev = import.meta.env.DEV
export const isProd = import.meta.env.PROD
export const isTest = import.meta.env.MODE === 'test'
