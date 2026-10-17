import apiClient from './client'

export type FeedbackStatus = 'open' | 'resolved' | 'closed'
export type FeedbackCategory = 'bug' | 'model' | 'access' | 'suggestion' | 'other'
/** 新的主题和回复一律是 public；private 只会出现在“取消私密”之前创建的历史内容上。 */
export type FeedbackVisibility = 'private' | 'public'
export interface FeedbackSummary {
  id: string; owner_id: string | null; owner_username: string | null
  title: string; category: FeedbackCategory; model: string | null; request_id: string | null
  status: FeedbackStatus; version: number; activity_seq: number; created_at: number; updated_at: number
  visibility?: FeedbackVisibility
  /** 人类可读的编号，用 #N 引用 */
  number?: number
  /** 管理员置顶的时间（秒），未置顶为 null */
  pinned_at?: number | null
  /** 标题、描述或类型被修改的时间（秒） */
  edited_at?: number | null
  /** 已删除（只会在增量列表里作为墓碑出现） */
  deleted_at?: number | null
}
export interface FeedbackIssue extends FeedbackSummary { description: string }
export type FeedbackEventKind = 'created' | 'reply' | 'status' | 'pin' | 'issue_edit' | 'issue_delete' | 'reply_edit' | 'reply_delete'
export interface FeedbackEvent {
  seq: number; issue_id: string; actor_id: string | null; actor_username: string | null
  actor_role: string; kind: FeedbackEventKind; content: string | null
  from_status: FeedbackStatus | null; to_status: FeedbackStatus | null; created_at: number
  visibility?: FeedbackVisibility
  /** reply_edit / reply_delete 标记事件所指向的回复 */
  target_seq?: number | null
  /** 回复被作者修改的时间（秒） */
  edited_at?: number | null
  /** 回复被删除的时间（秒），内容已清空 */
  deleted_at?: number | null
}
/** user_id -> 头像版本；没有头像的人不在其中 */
export type AvatarVersions = Record<string, number>
export interface FeedbackPage { items: FeedbackSummary[]; total: number; next_cursor: number | null; avatars?: AvatarVersions }
export interface FeedbackDetail { issue: FeedbackIssue; events: FeedbackEvent[]; next_event_cursor: number | null; has_more: boolean; avatars?: AvatarVersions }
export interface FeedbackCreate { title: string; description: string; category: FeedbackCategory; model?: string; request_id?: string; client_request_id: string; include_diagnostics?: boolean }
export type FeedbackAction = { action: 'reply'; content: string } | { action: 'set_status'; status: FeedbackStatus; comment?: string } | { action: 'set_pinned'; pinned: boolean }
  | { action: 'edit_issue'; title?: string; description?: string; category?: FeedbackCategory }
  | { action: 'delete_issue' }
  | { action: 'edit_reply'; seq: number; content: string }
  | { action: 'delete_reply'; seq: number }
export type FeedbackCommand = FeedbackAction & { expected_version: number; client_request_id: string; diagnostic_request_id?: string }
export interface FeedbackMutation { issue: FeedbackIssue; replayed: boolean }
const base = (admin: boolean) => admin ? '/api/admin/feedback' : '/api/feedback'
export async function listFeedback(admin: boolean, params: Record<string, string | number | undefined> = {}): Promise<FeedbackPage> {
  return (await apiClient.get<FeedbackPage>(base(admin), { params })).data
}
export async function getFeedback(admin: boolean, id: string, afterEvent = 0): Promise<FeedbackDetail> {
  return (await apiClient.get<FeedbackDetail>(`${base(admin)}/${encodeURIComponent(id)}`, { params: { after_event: afterEvent, limit: 20 } })).data
}
export async function createFeedback(input: FeedbackCreate, admin = false): Promise<FeedbackMutation> {
  return (await apiClient.post<FeedbackMutation>(base(admin), input)).data
}
export async function commandFeedback(admin: boolean, id: string, input: FeedbackCommand): Promise<FeedbackMutation> {
  return (await apiClient.post<FeedbackMutation>(`${base(admin)}/${encodeURIComponent(id)}/commands`, input)).data
}
/** 可被 @ 的管理员用户名（只含用户名） */
export async function listMentionableAdmins(admin: boolean): Promise<string[]> {
  return (await apiClient.get<{ items: string[] }>(`${base(admin)}/mentionable`)).data.items
}

export interface FeedbackNotification {
  id: number; kind: 'mention'; issue_id: string; issue_number: number; issue_title: string
  event_seq: number; actor_id: string | null; actor_username: string | null; snippet: string; created_at: number; read_at: number | null
}
export interface NotificationPage { items: FeedbackNotification[]; unread_count: number; next_before: number | null; avatars?: AvatarVersions }
/** 站内通知（被 @ 到）；管理员和普通用户都走用户路由，只返回自己的通知 */
export async function listNotifications(params: { unread?: boolean, before?: number, limit?: number } = {}): Promise<NotificationPage> {
  return (await apiClient.get<NotificationPage>('/api/feedback/notifications', { params: { unread: params.unread ? 1 : undefined, before: params.before, limit: params.limit } })).data
}
/** 标记已读：传 ids，或 { all: true }；返回仍未读的数量 */
export async function markNotificationsRead(target: { ids: number[] } | { all: true }): Promise<number> {
  return (await apiClient.post<{ unread_count: number }>('/api/feedback/notifications/read', target)).data.unread_count
}
