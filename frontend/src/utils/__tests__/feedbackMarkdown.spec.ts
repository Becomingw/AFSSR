import { describe, expect, it } from 'vitest'

import { renderFeedbackMarkdown } from '../feedbackMarkdown'

const options = { issueBase: '/admin/feedback', mentionable: ['Alice', 'ops.team'] }
const render = (source: string) => {
  const root = document.createElement('div')
  root.innerHTML = renderFeedbackMarkdown(source, options)
  return root
}

describe('renderFeedbackMarkdown', () => {
  it('renders common markdown', () => {
    const root = render('**bold** and `code`\n\n- one\n- two\n\n> quote')
    expect(root.querySelector('strong')?.textContent).toBe('bold')
    expect(root.querySelector('code')?.textContent).toBe('code')
    expect(root.querySelectorAll('li')).toHaveLength(2)
    expect(root.querySelector('blockquote')).toBeTruthy()
  })

  it('shows raw html literally instead of interpreting it', () => {
    const root = render('<script>bad()</script> <img src=x onerror=alert(1)>')
    expect(root.querySelector('script')).toBeNull()
    expect(root.querySelector('img')).toBeNull()
    expect(root.textContent).toContain('<script>bad()</script>')
  })

  it('renders https and http images and drops every other image source', () => {
    const ok = render('![diagram](https://example.com/a.png "t")')
    const img = ok.querySelector('img')!
    expect(img.getAttribute('src')).toBe('https://example.com/a.png')
    expect(img.getAttribute('alt')).toBe('diagram')
    expect(img.getAttribute('loading')).toBe('lazy')
    expect(img.getAttribute('referrerpolicy')).toBe('no-referrer')
    for (const bad of ['data:image/png;base64,AAAA', 'javascript:alert(1)', '//evil.test/x.png', '/local.png']) {
      expect(render(`![x](${bad})`).querySelector('img'), bad).toBeNull()
    }
  })

  it('keeps only safe link schemes and opens external links safely', () => {
    const root = render('[ok](https://example.com) [mail](mailto:a@b.co) [bad](javascript:alert(1))')
    const links = [...root.querySelectorAll('a[href]')]
    expect(links.map(a => a.getAttribute('href'))).toEqual(['https://example.com', 'mailto:a@b.co'])
    // The unsafe link keeps its text but loses the destination.
    expect(root.textContent).toContain('bad')
    expect(links[0].getAttribute('target')).toBe('_blank')
    expect(links[0].getAttribute('rel')).toContain('noopener')
  })

  it('links #number references to the in-app issue page, outside code and links', () => {
    const root = render('see #12 and #7, not `#99`, nor [#5](https://x.test), nor abc#3, nor &#39;')
    const refs = [...root.querySelectorAll('a.fb-ref')]
    expect(refs.map(a => [a.getAttribute('href'), a.textContent])).toEqual([
      ['/admin/feedback/12', '#12'],
      ['/admin/feedback/7', '#7'],
    ])
    expect(root.querySelector('code')?.textContent).toBe('#99')
  })

  it('highlights only known administrators and ignores email addresses', () => {
    const root = render('cc @alice @OPS.team, @nobody and mail me@alice.example.com')
    const mentions = [...root.querySelectorAll('.fb-mention')].map(node => node.textContent)
    expect(mentions).toEqual(['@alice', '@OPS.team'])
    expect(root.textContent).toContain('@nobody')
  })

  it('returns an empty string for blank input', () => {
    expect(renderFeedbackMarkdown('  \n ', options)).toBe('')
  })
})
