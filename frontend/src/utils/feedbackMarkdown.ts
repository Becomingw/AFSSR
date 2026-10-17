import { Marked, type Tokens } from 'marked'
import DOMPurify from 'dompurify'
// Registers the shared DOMPurify hooks (link/image attributes) used across the app.
import '@/utils/sanitize'

/**
 * 反馈内容的 Markdown 渲染。
 * - 原始 HTML 一律按文本显示（不执行、不解析），再经 DOMPurify 兜底；
 * - 图片只接受 http(s) 地址；链接只接受 http(s)/mailto/站内路径；
 * - `#12` 渲染为指向对应反馈的站内链接，`@name` 仅在 name 是已知管理员时高亮。
 */

const ALLOWED_TAGS = [
  'p', 'br', 'hr', 'strong', 'em', 'del', 'code', 'pre', 'blockquote',
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'ul', 'ol', 'li', 'a', 'img', 'span',
  'table', 'thead', 'tbody', 'tr', 'th', 'td', 'input',
]
const ALLOWED_ATTR = [
  'href', 'title', 'target', 'rel', 'class', 'src', 'alt', 'referrerpolicy',
  'loading', 'decoding', 'data-issue', 'type', 'checked', 'disabled',
]
const ALLOWED_URI_REGEXP = /^(?:https?:|mailto:|#|\/(?!\/))/i

const escapeHtml = (value: string) => value
  .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;').replace(/'/g, '&#39;')

const marked = new Marked({ gfm: true, breaks: true })
marked.use({
  renderer: {
    // Raw HTML in a feedback is shown literally, never interpreted.
    html(token: Tokens.HTML | Tokens.Tag) {
      return escapeHtml(token.text)
    },
  },
})

export interface FeedbackMarkdownOptions {
  /** 站内反馈页前缀，例如 `/admin/feedback` 或 `/dashboard/feedback` */
  issueBase: string
  /** 可被 @ 的管理员用户名（大小写不敏感） */
  mentionable?: readonly string[]
}

const WORD = '\\p{L}\\p{N}_'
const REFERENCE = new RegExp(`(^|[^${WORD}&#/])#(\\d{1,9})(?![${WORD}])`, 'gu')
const MENTION = new RegExp(`(^|[^${WORD}])@([${WORD}.-]{1,64})`, 'gu')

function skipText(node: Node): boolean {
  for (let current = node.parentElement; current; current = current.parentElement) {
    const tag = current.tagName
    if (tag === 'A' || tag === 'CODE' || tag === 'PRE') return true
  }
  return false
}

interface Piece { index: number, length: number, node: HTMLElement }

function decorateText(text: Text, options: FeedbackMarkdownOptions, known: Set<string>) {
  const value = text.data
  const pieces: Piece[] = []
  for (const match of value.matchAll(REFERENCE)) {
    const start = (match.index ?? 0) + match[1].length
    const link = document.createElement('a')
    link.setAttribute('href', `${options.issueBase}/${match[2]}`)
    link.setAttribute('data-issue', match[2])
    link.className = 'fb-ref'
    link.textContent = `#${match[2]}`
    pieces.push({ index: start, length: match[2].length + 1, node: link })
  }
  for (const match of value.matchAll(MENTION)) {
    const name = match[2].replace(/[.-]+$/, '')
    if (!name || !known.has(name.toLowerCase())) continue
    const span = document.createElement('span')
    span.className = 'fb-mention'
    span.textContent = `@${name}`
    pieces.push({ index: (match.index ?? 0) + match[1].length, length: name.length + 1, node: span })
  }
  if (!pieces.length) return
  pieces.sort((a, b) => a.index - b.index)
  const fragment = document.createDocumentFragment()
  let cursor = 0
  for (const piece of pieces) {
    if (piece.index < cursor) continue
    if (piece.index > cursor) fragment.append(value.slice(cursor, piece.index))
    fragment.append(piece.node)
    cursor = piece.index + piece.length
  }
  if (cursor < value.length) fragment.append(value.slice(cursor))
  text.replaceWith(fragment)
}

export function renderFeedbackMarkdown(source: string, options: FeedbackMarkdownOptions): string {
  if (!source.trim()) return ''
  const raw = marked.parse(source, { async: false }) as string
  const fragment = DOMPurify.sanitize(raw, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    ALLOWED_URI_REGEXP,
    RETURN_DOM_FRAGMENT: true,
  }) as unknown as DocumentFragment

  for (const image of Array.from(fragment.querySelectorAll('img'))) {
    if (!/^https?:\/\//i.test(image.getAttribute('src') || '')) {
      image.replaceWith(document.createTextNode(image.getAttribute('alt') || ''))
    }
  }
  for (const input of Array.from(fragment.querySelectorAll('input'))) {
    if (input.getAttribute('type') !== 'checkbox') input.remove()
    else input.setAttribute('disabled', '')
  }
  for (const link of Array.from(fragment.querySelectorAll('a[href]'))) {
    if (/^https?:\/\//i.test(link.getAttribute('href') || '')) {
      link.setAttribute('target', '_blank')
      link.setAttribute('rel', 'noopener noreferrer')
    }
  }

  const known = new Set((options.mentionable ?? []).map(name => name.toLowerCase()))
  const walker = document.createTreeWalker(fragment, NodeFilter.SHOW_TEXT)
  const texts: Text[] = []
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!skipText(node)) texts.push(node as Text)
  }
  for (const text of texts) decorateText(text, options, known)

  const wrapper = document.createElement('div')
  wrapper.appendChild(fragment)
  return wrapper.innerHTML.trim()
}
