import apiClient from './client'

export type FeedbackStatus = 'open' | 'resolved' | 'closed'
export type FeedbackCategory = 'bug' | 'model' | 'access' | 'suggestion' | 'other'
export type FeedbackVisibility = 'private' | 'public'
export interface FeedbackSummary {
  id: string; owner_id: string | null; owner_username: string | null
  title: string; category: FeedbackCategory; model: string | null; request_id: string | null
  status: FeedbackStatus; version: number; activity_seq: number; created_at: number; updated_at: number
  visibility?: FeedbackVisibility
}
export interface FeedbackIssue extends FeedbackSummary { description: string }
export interface FeedbackEvent {
  seq: number; issue_id: string; actor_id: string | null; actor_username: string | null
  actor_role: string; kind: 'created' | 'reply' | 'status'; content: string | null
  from_status: FeedbackStatus | null; to_status: FeedbackStatus | null; created_at: number
  visibility?: FeedbackVisibility
}
export interface FeedbackPage { items: FeedbackSummary[]; total: number; next_cursor: number | null }
export interface FeedbackDetail { issue: FeedbackIssue; events: FeedbackEvent[]; next_event_cursor: number | null; has_more: boolean }
export interface FeedbackCreate { title: string; description: string; category: FeedbackCategory; model?: string; request_id?: string; client_request_id: string; include_diagnostics?: boolean; visibility?: FeedbackVisibility }
export type FeedbackAction = { action: 'reply'; content: string } | { action: 'set_status'; status: FeedbackStatus; comment?: string }
export type FeedbackCommand = FeedbackAction & { expected_version: number; client_request_id: string; diagnostic_request_id?: string; visibility?: FeedbackVisibility }
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
