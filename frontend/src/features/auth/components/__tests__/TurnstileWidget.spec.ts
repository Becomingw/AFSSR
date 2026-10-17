import { afterEach, describe, expect, it } from 'vitest'
import { createApp } from 'vue'
import TurnstileWidget from '../TurnstileWidget.vue'

function mountTurnstileWidget() {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(TurnstileWidget, { siteKey: 'site-public-key' })
  const instance = app.mount(root) as unknown as {
    execute: (action: string) => Promise<string>
  }
  return {
    instance,
    unmount: () => {
      app.unmount()
      root.remove()
    },
  }
}

function turnstileScripts() {
  return Array.from(
    document.querySelectorAll<HTMLScriptElement>('script[data-arca-turnstile="true"]')
  )
}

describe('TurnstileWidget script loading', () => {
  afterEach(() => {
    document.body.innerHTML = ''
    document.head.querySelectorAll('script[data-arca-turnstile="true"]').forEach((script) => {
      script.remove()
    })
    delete (window as unknown as { turnstile?: unknown }).turnstile
    delete (window as unknown as { __arcaTurnstileScriptPromise?: Promise<void> })
      .__arcaTurnstileScriptPromise
  })

  it('retries loading the Turnstile script after a transient load failure', async () => {
    const mounted = mountTurnstileWidget()

    const firstAttempt = mounted.instance.execute('register')
    const firstScript = turnstileScripts()[0]
    expect(firstScript).toBeTruthy()
    firstScript.dispatchEvent(new Event('error'))
    await expect(firstAttempt).rejects.toThrow('Turnstile script failed')

    const secondAttempt = mounted.instance.execute('register')
    const scriptsAfterRetry = turnstileScripts()
    expect(scriptsAfterRetry).toHaveLength(1)
    expect(scriptsAfterRetry[0]).not.toBe(firstScript)
    scriptsAfterRetry[0].dispatchEvent(new Event('error'))
    await expect(secondAttempt).rejects.toThrow('Turnstile script failed')

    mounted.unmount()
  })
})
