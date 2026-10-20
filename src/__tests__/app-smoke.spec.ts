import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { describe, expect, it } from 'vitest'

import App from '../App.vue'
import { i18n } from '../i18n'
import { router } from '../router'

describe('App shell outside Tauri', () => {
  it('renders the desktop-required notice and never fake data', async () => {
    const wrapper = mount(App, {
      global: {
        plugins: [createPinia(), router, i18n],
      },
    })
    await router.isReady()
    await flushPromises()

    const text = wrapper.text()
    expect(text).toContain('需要桌面应用')
    expect(text).toContain('pnpm tauri dev')
    // No dashboard/agent placeholder content should be rendered in a browser.
    expect(text).not.toContain('仪表盘')

    wrapper.unmount()
  })
})
