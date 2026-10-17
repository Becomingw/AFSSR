<template>
  <PageContainer class="feedback-page">
    <div class="feedback-toolbar">
      <button
        v-if="selectedId"
        type="button"
        class="back-link"
        :disabled="busy"
        @click="showList"
      >
        <ArrowLeft :size="16" />所有主题
      </button>
      <h1 v-else>
        讨论与反馈
      </h1>
      <div class="toolbar-actions">
        <Button
          variant="outline"
          size="icon"
          title="刷新"
          :disabled="loading || detailLoading || busy"
          @click="refresh"
        >
          <RefreshCw :size="16" />
        </Button>
        <Button
          v-if="canCreate"
          @click="creating = true"
        >
          <Plus
            :size="16"
            class="mr-2"
          />发起主题
        </Button>
      </div>
    </div>
    <p
      v-if="error"
      role="alert"
      class="feedback-error"
    >
      {{ error }}
    </p>
    <p
      v-if="notice"
      role="status"
      class="feedback-notice"
    >
      {{ notice }}
    </p>

    <template v-if="!selectedId">
      <form
        class="list-filters"
        @submit.prevent="filterChanged"
      >
        <div class="search-field">
          <Input
            v-model="search"
            aria-label="搜索主题"
            placeholder="搜索主题标题"
            :maxlength="200"
          /><Button
            variant="ghost"
            type="submit"
            size="icon"
            title="搜索"
          >
            <Search :size="16" />
          </Button>
        </div>
        <select
          v-model="category"
          aria-label="主题类型"
          @change="filterChanged"
        >
          <option value="">
            全部类型
          </option><option
            v-for="option in categories"
            :key="option.value"
            :value="option.value"
          >
            {{ option.label }}
          </option>
        </select>
      </form>
      <div
        class="status-tabs"
        role="group"
        aria-label="主题状态"
      >
        <button
          v-for="option in statuses"
          :key="option.value"
          type="button"
          :aria-pressed="status === option.value"
          @click="status = option.value; filterChanged()"
        >
          {{ option.label }}
        </button>
      </div>
      <section
        aria-label="主题列表"
        class="issue-list"
      >
        <p
          v-if="loading"
          role="status"
          class="empty-state"
        >
          加载中…
        </p>
        <p
          v-else-if="!items.length"
          class="empty-state"
        >
          暂无主题
        </p>
        <template v-else>
          <div
            class="list-heading"
            aria-hidden="true"
          >
            <span>主题</span><span>类型</span><span>更新时间</span>
          </div>
          <ul>
            <li
              v-for="issue in items"
              :key="issue.id"
            >
              <button
                type="button"
                class="issue-row"
                @click="openIssue(issue.id)"
              >
                <component
                  :is="statusIcon(issue.status)"
                  :size="18"
                  class="issue-status-icon"
                  :class="[`status-${issue.status}`]"
                />
                <span class="issue-main"><strong><Pin
                  v-if="issue.pinned_at"
                  :size="13"
                  class="pinned-mark"
                  aria-label="已置顶"
                />{{ issue.title }}</strong><span>{{ feedbackRef(issue) }} · {{ issue.owner_username || '已注销用户' }} · {{ feedbackStatusLabel(issue.status) }}<template v-if="issue.visibility === 'private'"> · <Lock
                  :size="11"
                  class="legacy-lock"
                  aria-hidden="true"
                />历史私密</template></span></span>
                <span class="row-category">{{ feedbackCategoryLabel(issue.category) }}</span>
                <time class="row-date">{{ feedbackDate(issue.updated_at) }}</time><ChevronRight
                  :size="15"
                  class="row-chevron"
                />
              </button>
            </li>
          </ul>
        </template>
      </section>
      <div class="list-pagination">
        <span>{{ total }} 个主题 · 第 {{ page + 1 }} 页</span><div>
          <Button
            variant="outline"
            size="icon"
            title="上一页"
            :disabled="page === 0 || loading"
            @click="page--; loadList()"
          >
            <ChevronLeft :size="16" />
          </Button><Button
            variant="outline"
            size="icon"
            title="下一页"
            :disabled="(page + 1) * 20 >= total || loading"
            @click="page++; loadList()"
          >
            <ChevronRight :size="16" />
          </Button>
        </div>
      </div>
      <FeedbackDiagnostics
        v-model="diagnosticId"
        :admin="admin"
        :can-create="canCreate"
        @create="createFromDiagnostic"
      />
    </template>

    <p
      v-else-if="detailLoading"
      role="status"
      class="empty-state"
    >
      正在加载讨论…
    </p>
    <template v-else-if="detail">
      <header class="issue-heading">
        <div class="heading-row">
          <h1>{{ detail.issue.title }} <span>{{ feedbackRef(detail.issue) }}</span></h1>
          <Button
            v-if="manager"
            variant="outline"
            size="sm"
            :disabled="busy"
            @click="togglePin"
          >
            <component
              :is="detail.issue.pinned_at ? PinOff : Pin"
              :size="14"
              class="mr-2"
            />{{ detail.issue.pinned_at ? '取消置顶' : '置顶' }}
          </Button>
          <Button
            v-if="canManageIssue"
            variant="outline"
            size="sm"
            :disabled="busy"
            @click="confirmingDelete = true"
          >
            <Trash2
              :size="14"
              class="mr-2"
            />删除主题
          </Button>
        </div>
        <div
          v-if="confirmingDelete && canManageIssue"
          class="issue-delete-confirm"
          role="alertdialog"
          aria-label="确认删除主题"
        >
          <span>删除后这个主题及其全部回复对所有人不可见，编号不会被复用，且无法恢复。</span>
          <Button
            variant="destructive"
            size="sm"
            :disabled="busy"
            @click="removeIssue"
          >
            确认删除
          </Button>
          <Button
            variant="ghost"
            size="sm"
            :disabled="busy"
            @click="confirmingDelete = false"
          >
            取消
          </Button>
        </div>
        <div class="issue-meta">
          <span
            class="status-badge"
            :class="[`status-${detail.issue.status}`]"
          ><component
            :is="statusIcon(detail.issue.status)"
            :size="14"
          />{{ feedbackStatusLabel(detail.issue.status) }}</span><span
            v-if="legacyPrivate"
            class="legacy-badge"
            title="这是取消私密主题之前创建的主题，仍只有发起人和管理员能看到"
          ><Lock :size="13" />历史私密主题</span><span
            v-if="detail.issue.pinned_at"
            class="pin-badge"
          ><Pin :size="13" />已置顶</span><span>{{ detail.issue.owner_username || '已注销用户' }} 发起于 {{ feedbackDate(detail.issue.created_at) }}</span>
        </div>
      </header>
      <div class="issue-layout">
        <section
          class="issue-discussion"
          aria-label="主题详情"
        >
          <FeedbackDiscussion
            :issue="detail.issue"
            :events="detail.events"
            :issue-base="base"
            :mentionable="mentionable"
            :current-user-id="currentUserId"
            :manager="manager"
            :busy="busy"
            :actions="{ editIssue, editReply, deleteReply }"
            @reply="replyTo"
          />
          <Button
            v-if="detail.has_more"
            variant="outline"
            :disabled="busy"
            @click="moreEvents"
          >
            加载更多历史
          </Button>
          <form
            class="reply-composer"
            @submit.prevent="reply"
          >
            <div class="composer-heading">
              <label for="feedback-reply">发表评论</label><span
                v-if="replyingTo"
                class="reply-target"
              >回复 {{ replyingTo }}<button
                type="button"
                title="取消指定回复"
                @click="replyingTo = ''"
              ><X :size="13" /></button></span>
            </div>
            <FeedbackEditor
              id="feedback-reply"
              ref="replyInput"
              v-model="replyText"
              aria-label="回复"
              placeholder="补充信息、回复讨论，或填写最终解决说明…（支持 Markdown）"
              :maxlength="20000"
              :disabled="busy"
              :issue-base="base"
              :mentionable="mentionable"
            />
            <label
              v-if="diagnosticId.trim()"
              class="diagnostic-opt-in"
            ><input
              v-model="attachReply"
              type="checkbox"
              :disabled="busy"
            >附上该请求安全诊断摘要（将公开显示）</label>
            <div class="composer-actions">
              <span class="private-reply"><Globe :size="13" />公开回复 · 所有已登录用户可见</span>
              <div class="submit-actions">
                <Button
                  v-if="canManageIssue && detail.issue.status !== 'resolved'"
                  type="button"
                  variant="outline"
                  :disabled="busy || !replyText.trim()"
                  @click="resolve"
                >
                  <CheckCircle2
                    :size="15"
                    class="mr-2"
                  />评论并标记已解决
                </Button><Button
                  type="submit"
                  :disabled="busy || !replyText.trim()"
                >
                  <Send
                    :size="15"
                    class="mr-2"
                  />发送回复
                </Button>
              </div>
            </div>
            <p class="privacy-hint">
              所有回复都是公开的，请勿包含密钥、密码或私人对话。用 @用户名 可以提醒对方。
            </p>
          </form>
          <div
            v-if="canManageIssue"
            class="lifecycle-actions"
          >
            <span>解决需附说明；关闭仅限管理员。</span><div>
              <Button
                v-if="detail.issue.status !== 'open'"
                variant="ghost"
                :disabled="busy"
                @click="reopen"
              >
                <RotateCcw
                  :size="14"
                  class="mr-2"
                />重新开启
              </Button><Button
                v-if="manager && detail.issue.status !== 'closed'"
                variant="ghost"
                :disabled="busy"
                @click="closing = true"
              >
                <CircleX
                  :size="14"
                  class="mr-2"
                />关闭主题
              </Button>
            </div>
          </div>
        </section>
        <aside
          class="issue-sidebar"
          aria-label="主题信息"
        >
          <h2>主题信息</h2><dl>
            <div><dt>类型</dt><dd>{{ feedbackCategoryLabel(detail.issue.category) }}</dd></div><div v-if="detail.issue.model">
              <dt>模型</dt><dd>{{ detail.issue.model }}</dd>
            </div><div v-if="detail.issue.request_id">
              <dt>关联请求</dt><dd class="request-reference">
                {{ detail.issue.request_id }}
              </dd>
            </div><div><dt>可见范围</dt><dd>{{ legacyPrivate ? '发起人与管理员（历史私密）' : '所有已登录用户' }}</dd></div><div><dt>最后更新</dt><dd>{{ feedbackDate(detail.issue.updated_at) }}</dd></div>
          </dl>
          <FeedbackDiagnostics
            v-model="diagnosticId"
            :admin="admin"
            :can-create="canCreate"
            @create="createFromDiagnostic"
          />
        </aside>
      </div>
    </template>

    <Dialog
      :open="creating"
      title="发起主题"
      :persistent="busy"
      @update:open="creating = $event"
    >
      <form
        class="feedback-create-form"
        @submit.prevent="submit"
      >
        <label>标题<Input
          v-model="draft.title"
          :maxlength="200"
          :disabled="busy"
          required
        /></label>
        <div class="form-pair">
          <label>问题类型<select
            v-model="draft.category"
            :disabled="busy"
          ><option
            v-for="option in categories"
            :key="option.value"
            :value="option.value"
          >{{ option.label }}</option></select></label>
        </div>
        <div class="form-pair">
          <label>模型（可选）<Input
            v-model="draft.model"
            :maxlength="200"
            :disabled="busy"
          /></label><label>请求 ID（可选）<Input
            v-model="draft.request_id"
            :maxlength="128"
            :disabled="busy"
          /></label>
        </div>
        <div class="editor-field">
          <span class="editor-field-heading">主题内容<button
            type="button"
            class="focus-toggle"
            :disabled="busy"
            title="专注模式：全屏书写，Esc 退出"
            @click="focusing = true"
          ><Maximize2 :size="13" />专注模式</button></span>
          <FeedbackEditor
            v-model="draft.description"
            aria-label="主题内容"
            placeholder="现象、复现步骤与预期结果；请勿包含密钥、密码或私人对话。用 @用户名 可以提醒对方"
            :maxlength="20000"
            :disabled="busy"
            required
            :issue-base="base"
            :mentionable="mentionable"
          />
        </div>
        <label
          v-if="draft.request_id.trim()"
          class="diagnostic-opt-in"
        ><input
          v-model="includeDiagnostics"
          type="checkbox"
          :disabled="busy"
        >附上安全诊断摘要</label>
        <p class="privacy-hint">
          主题、回复、模型及关联请求标识对所有已登录用户公开；附上的诊断摘要也会公开显示。
        </p>
        <p
          v-if="draftError"
          role="alert"
          class="feedback-error"
        >
          {{ draftError }}
        </p>
        <div class="dialog-actions">
          <Button
            type="button"
            variant="outline"
            :disabled="busy"
            @click="creating = false"
          >
            取消
          </Button><Button
            type="submit"
            :disabled="busy || !draft.title.trim() || !draft.description.trim()"
          >
            发起
          </Button>
        </div>
      </form>
    </Dialog>
    <FocusComposer
      v-model:title="draft.title"
      v-model:description="draft.description"
      v-model:category="draft.category"
      :open="focusing && creating"
      :busy="busy"
      :categories="categories"
      :issue-base="base"
      :mentionable="mentionable"
      :error="draftError"
      @close="focusing = false"
      @submit="submitFromFocus"
    />
    <Dialog
      :open="closing"
      title="关闭主题"
      size="sm"
      :persistent="busy"
      @update:open="closing = $event"
    >
      <div class="close-confirmation">
        <strong>{{ detail?.issue.title }}</strong><p>关闭不等同于解决。已确认修复的问题，请附解决说明并标记已解决。</p><div class="dialog-actions">
          <Button
            variant="outline"
            :disabled="busy"
            @click="closing = false"
          >
            取消
          </Button><Button
            :disabled="busy || !manager"
            @click="closeIssue"
          >
            确认关闭
          </Button>
        </div>
      </div>
    </Dialog>
  </PageContainer>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { ArrowLeft, CheckCircle2, ChevronLeft, ChevronRight, CircleDot, CircleX, Globe, Lock, Maximize2, Pin, PinOff, Plus, RefreshCw, RotateCcw, Search, Send, Trash2, X } from 'lucide-vue-next'
import PageContainer from '@/components/layout/PageContainer.vue'
import { Button, Input } from '@/components/ui'
import Dialog from '@/components/ui/dialog/Dialog.vue'
import FeedbackDiscussion from './FeedbackDiscussion.vue'
import FeedbackDiagnostics from './FeedbackDiagnostics.vue'
import FeedbackEditor from './components/FeedbackEditor.vue'
import FocusComposer from './components/FocusComposer.vue'
import { useFeedback } from './useFeedback'
import { feedbackStatuses as statuses, feedbackCategories as categories, feedbackDate, feedbackRef, feedbackStatusLabel, feedbackCategoryLabel } from './feedback-view'
const { admin, manager, canCreate, selectedId, status, category, search, page, total, items, detail, loading, detailLoading, busy, creating, closing, error, notice, draftError, replyText, replyingTo, replyInput, mentionable, base, togglePin, draft, diagnosticId, includeDiagnostics, attachReply, canManageIssue, currentUserId, editIssue, deleteIssue, editReply, deleteReply, legacyPrivate, createFromDiagnostic, loadList, refresh, filterChanged, openIssue, showList, replyTo, moreEvents, submit, reply, resolve, reopen, closeIssue } = useFeedback()
const confirmingDelete = ref(false)
const focusing = ref(false)
// Submitting from focus mode leaves it once the topic was created (the dialog closes with it).
async function submitFromFocus() { await submit(); if (!creating.value) focusing.value = false }
async function removeIssue() { if (await deleteIssue()) confirmingDelete.value = false }
function statusIcon(value: string) { return value === 'open' ? CircleDot : value === 'resolved' ? CheckCircle2 : CircleX }
</script>

<style scoped>
.feedback-page{max-width:1320px;margin:0 auto;min-width:0}.feedback-toolbar,.toolbar-actions,.back-link,.list-filters,.search-field,.list-pagination,.list-pagination>div,.issue-meta,.issue-meta>span,.composer-heading,.reply-target,.composer-actions,.submit-actions,.private-reply,.lifecycle-actions,.lifecycle-actions>div,.dialog-actions{display:flex;align-items:center;gap:10px}.feedback-toolbar{justify-content:space-between;margin-bottom:26px;flex-wrap:wrap}.feedback-toolbar h1{font-size:24px;font-weight:600;line-height:1.4}.toolbar-actions{margin-left:auto}.back-link{font-size:13px;color:var(--muted-foreground)}.back-link:hover{color:var(--foreground)}.feedback-error,.feedback-notice{font-size:13px;line-height:1.6;margin:12px 0;overflow-wrap:anywhere}.feedback-error{color:var(--destructive)}.feedback-notice{color:var(--primary)}.list-filters{justify-content:space-between;flex-wrap:wrap;margin-bottom:18px}.search-field{flex:1;max-width:420px;min-width:190px;position:relative;gap:0}.search-field>button{position:absolute;right:0}.search-field :deep(input){padding-right:42px}.feedback-page select{height:36px;border:1px solid var(--border);border-radius:5px;background:var(--background);padding:0 10px;font-size:13px;max-width:100%;min-width:0}.status-tabs{display:flex;gap:24px;border-bottom:1px solid var(--border);margin-bottom:20px;overflow:auto}.status-tabs button{font-size:13px;color:var(--muted-foreground);padding:11px 0;border-bottom:2px solid transparent;white-space:nowrap}.status-tabs button[aria-pressed=true]{color:var(--foreground);border-bottom-color:var(--primary);font-weight:600}.issue-list{border:1px solid var(--border);border-radius:6px;overflow:hidden}.issue-list ul{list-style:none;margin:0;padding:0}.issue-list li+li{border-top:1px solid var(--border)}.list-heading{display:grid;grid-template-columns:minmax(0,1fr) 100px 150px;gap:16px;padding:12px 49px 12px 52px;font-size:11px;color:var(--muted-foreground);background:var(--muted);border-bottom:1px solid var(--border)}.issue-row{display:grid;grid-template-columns:18px minmax(0,1fr) 100px 150px 15px;gap:16px;align-items:center;min-height:86px;width:100%;padding:18px;text-align:left}.issue-row:hover{background:color-mix(in srgb, var(--muted) 65%, transparent)}.issue-main{display:grid;gap:7px;min-width:0}.issue-main strong{font-size:14px;font-weight:600;overflow-wrap:anywhere}.issue-main>span,.row-category,.row-visibility,.row-date{font-size:11px;color:var(--muted-foreground);overflow-wrap:anywhere}.row-visibility{display:flex;gap:5px;align-items:center}.row-chevron{color:var(--muted-foreground)}.status-open{color:var(--primary)}.status-resolved{color:var(--feedback-resolved,#6e56b0)}.status-closed{color:var(--muted-foreground)}.empty-state{font-size:13px;color:var(--muted-foreground);text-align:center;padding:60px 12px}.list-pagination{justify-content:space-between;font-size:12px;color:var(--muted-foreground);margin-top:16px}.issue-heading{border-bottom:1px solid var(--border);padding-bottom:22px;margin-bottom:28px}.issue-heading h1{font-size:24px;line-height:1.5;font-weight:600;overflow-wrap:anywhere}.issue-heading h1>span{font-weight:400;color:var(--muted-foreground);font-size:18px;white-space:nowrap}.issue-meta{margin-top:12px;font-size:12px;color:var(--muted-foreground);gap:14px;flex-wrap:wrap}.status-badge{padding:4px 8px;border:1px solid currentColor;border-radius:5px;font-size:11px;gap:5px!important}.issue-layout{display:grid;grid-template-columns:minmax(0,1fr) 230px;gap:28px;align-items:start}.issue-discussion,.issue-sidebar{min-width:0}.issue-sidebar{border-left:1px solid var(--border);padding-left:22px}.issue-sidebar h2{font-size:12px;font-weight:600;margin-bottom:20px}.issue-sidebar dl{display:grid;gap:18px}.issue-sidebar dt{font-size:11px;color:var(--muted-foreground);margin-bottom:6px}.issue-sidebar dd{font-size:12px;line-height:1.6;overflow-wrap:anywhere}.request-reference{font-family:monospace}.reply-composer{border-top:1px solid var(--border);padding-top:24px;margin-top:12px}.composer-heading{justify-content:space-between;margin-bottom:12px;font-size:14px;font-weight:600;flex-wrap:wrap}.reply-target{font-weight:400;font-size:12px;color:var(--muted-foreground)}.reply-target button{display:flex;padding:4px}.feedback-page textarea{display:block;width:100%;min-height:140px;border:1px solid var(--border);border-radius:6px;background:var(--background);padding:12px 14px;font-size:14px;line-height:24px;resize:vertical}.feedback-page textarea:focus-visible,.feedback-page select:focus-visible,.back-link:focus-visible,.issue-row:focus-visible,.status-tabs button:focus-visible{outline:2px solid var(--ring);outline-offset:2px}.composer-actions{justify-content:space-between;flex-wrap:wrap;margin-top:12px;gap:12px}.submit-actions{flex-wrap:wrap;justify-content:flex-end;margin-left:auto}.private-reply,.privacy-hint{font-size:11px;color:var(--muted-foreground);line-height:1.8}.privacy-hint{margin-top:12px}.lifecycle-actions{justify-content:space-between;flex-wrap:wrap;padding-top:18px;margin-top:20px;border-top:1px solid var(--border);gap:8px}.lifecycle-actions>span{font-size:11px;color:var(--muted-foreground)}.diagnostic-opt-in{display:flex!important;align-items:center;gap:8px!important;font-size:12px!important;font-weight:400!important;margin-top:12px}.diagnostic-opt-in input{width:15px;height:15px;accent-color:var(--primary)}.feedback-create-form{display:grid;gap:18px}.feedback-create-form label{display:grid;gap:8px;font-size:13px}.form-pair{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:16px}.feedback-create-form select{width:100%}.dialog-actions{justify-content:flex-end}.close-confirmation{display:grid;gap:16px;font-size:13px;line-height:1.7;overflow-wrap:anywhere}.close-confirmation p{color:var(--muted-foreground)}
:global(.dark) .feedback-page,:global(body[theme-mode='dark']) .feedback-page{--feedback-resolved:#c4a7ee}
@media(max-width:1100px){.issue-layout{grid-template-columns:minmax(0,1fr)} .issue-sidebar{grid-row:auto;border-left:0;padding:0 0 22px;border-bottom:1px solid var(--border)}.issue-sidebar dl{grid-template-columns:repeat(3,minmax(0,1fr));gap:14px}.issue-sidebar h2{margin-bottom:14px}.list-heading,.row-category,.row-date{display:none}.issue-row{grid-template-columns:18px minmax(0,1fr) 72px 15px}}
@media(max-width:640px){.feedback-toolbar{margin-bottom:20px}.feedback-toolbar h1,.issue-heading h1{font-size:21px}.issue-heading h1>span{font-size:14px}.issue-row{grid-template-columns:18px minmax(0,1fr) 15px;gap:10px;padding:14px}.row-chevron{grid-column:3;grid-row:1}.issue-sidebar dl{grid-template-columns:repeat(2,minmax(0,1fr))}.issue-meta{gap:10px}.form-pair{grid-template-columns:minmax(0,1fr)}.submit-actions{width:100%;justify-content:flex-start;margin-left:0}.submit-actions :deep(button){font-size:12px}.lifecycle-actions>div{flex-wrap:wrap}.status-tabs{gap:20px}}
.heading-row{display:flex;align-items:flex-start;justify-content:space-between;gap:16px}.heading-row h1{min-width:0}.pin-badge,.pinned-mark{color:var(--primary)}.pin-badge{display:inline-flex;align-items:center;gap:4px}.pinned-mark{display:inline-block;vertical-align:-1px;margin-right:6px;transform:rotate(35deg)}.editor-field{display:grid;gap:8px;font-size:13px}.editor-field-heading{display:flex;align-items:center;justify-content:space-between}.focus-toggle{display:inline-flex;align-items:center;gap:5px;font-size:12px;color:var(--muted-foreground);border:1px solid var(--border);border-radius:5px;padding:3px 9px}.focus-toggle:hover:not(:disabled){color:var(--foreground);background:var(--muted)}.focus-toggle:focus-visible{outline:2px solid var(--ring)}.legacy-badge,.legacy-lock{color:var(--muted-foreground)}.legacy-badge{display:inline-flex;align-items:center;gap:4px}.legacy-lock{display:inline;vertical-align:-1px;margin-right:3px}.heading-row{flex-wrap:wrap}.issue-delete-confirm{display:flex;flex-wrap:wrap;align-items:center;gap:10px;margin-top:14px;padding:10px 14px;font-size:12px;border:1px solid color-mix(in srgb, var(--destructive, #dc2626) 40%, transparent);border-radius:6px;background:color-mix(in srgb, var(--destructive, #dc2626) 8%, transparent)}
</style>
