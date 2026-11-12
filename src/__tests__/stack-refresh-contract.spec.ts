import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import StackView from '../pages/StackView.vue'
import { i18n } from '../i18n'
import { router } from '../router'
import type { SpendingOverview, StackItemView } from '../lib/types'

/**
 * M1-C.1 Final Contract Alignment.
 *
 * The store caches the spending overview for a 15s TTL, but a TTL is a *lazy*
 * rule: it only decides what the next read does, it never triggers a read on
 * its own. These tests pin the resulting user-visible contract so the copy and
 * the behaviour can never drift apart again:
 *
 *   - a page left untouched does NOT update itself (no invented polling);
 *   - a real activation event (window focus, re-entry) re-checks and picks up
 *     an external write once the TTL has expired;
 *   - an activation inside the TTL is a free cache hit;
 *   - the GUI's own writes update immediately, without waiting for the TTL.
 */

const fixtures = vi.hoisted(() => {
  const item = (overrides: Record<string, unknown> = {}): Record<string, unknown> => ({
    id: 'st_item',
    name: 'Codex',
    vendor: 'OpenAI',
    source: 'detected',
    detectionState: 'installed',
    confirmed: false,
    subscriptionState: 'registered',
    createdAt: '2026-11-11T00:00:00Z',
    updatedAt: '2026-11-11T00:00:00Z',
    detectionCount: 1,
    subscriptionCount: 2,
    ...overrides,
  })

  const spending = (overrides: Record<string, unknown> = {}): Record<string, unknown> => ({
    byCurrency: [
      {
        currency: 'USD',
        monthlyEquivalentMinor: 3000,
        annualEquivalentMinor: 36000,
        subscriptionCount: 1,
        hybridCount: 0,
      },
    ],
    upcomingRenewals: [],
    renewalsUnknown: [],
    staleRenewals: [],
    laterRenewals: [],
    incomplete: [],
    usageWithoutBills: [],
    counts: { total: 1, active: 1, paused: 0, cancelled: 0, expired: 0, unknown: 0 },
    generatedAt: '2026-11-11T00:00:00Z',
    upcomingWindowDays: 30,
    ...overrides,
  })

  const apiMock = {
    stackList: vi.fn(async () => [item()]),
    stackGet: vi.fn(async () => ({
      item: item(),
      subscriptions: [],
      detections: [],
    })),
    stackUpsertItem: vi.fn(async () => item()),
    stackConfirmItem: vi.fn(async () => item()),
    stackMergeItems: vi.fn(async () => item()),
    stackUnmergeItem: vi.fn(async () => item()),
    stackDeleteItem: vi.fn(async () => undefined),
    stackRefreshDetections: vi.fn(async () => ({
      detectionsTotal: 1,
      itemsCreated: 0,
      itemsAttached: 1,
      ranAt: '2026-11-11T00:00:00Z',
    })),
    subscriptionUpsert: vi.fn(async () => ({})),
    subscriptionDelete: vi.fn(async () => undefined),
    spendingOverview: vi.fn(async () => spending()),
  }

  return { item, spending, apiMock }
})

const apiMock = fixtures.apiMock
const item = fixtures.item as unknown as (overrides?: Record<string, unknown>) => StackItemView
const spending = fixtures.spending as unknown as (overrides?: Record<string, unknown>) => SpendingOverview

vi.mock('../lib/api', () => ({
  api: fixtures.apiMock,
  normalizeError: (error: unknown, fallback: string) =>
    error instanceof Error ? error.message : fallback,
  isDesktop: () => true,
  NOT_TAURI_MESSAGE: '',
  ApiError: class ApiError extends Error {
    code: string | null
    constructor(message: string, code: string | null = null) {
      super(message)
      this.code = code
    }
  },
}))

const TTL_MS = 15_000

function mountStack(): ReturnType<typeof mount> {
  return mount(StackView, {
    global: { plugins: [createPinia(), router, i18n], stubs: { teleport: true } },
    attachTo: document.body,
  })
}

const spendingReads = (): number => apiMock.spendingOverview.mock.calls.length

/** Advance both the mocked clock and the timers, then let promises settle. */
async function advance(ms: number): Promise<void> {
  vi.advanceTimersByTime(ms)
  await flushPromises()
}

async function mountAt(path = '/stack'): Promise<ReturnType<typeof mount>> {
  await router.push(path)
  await router.isReady()
  const wrapper = mountStack()
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.useFakeTimers()
  apiMock.stackList.mockClear()
  apiMock.stackList.mockImplementation(async () => [fixtures.item()] as never)
  apiMock.spendingOverview.mockClear()
  apiMock.spendingOverview.mockImplementation(async () => fixtures.spending() as never)
})

afterEach(() => {
  vi.useRealTimers()
  document.body.innerHTML = ''
})

describe('M1-C.1 refresh contract: what actually triggers a read', () => {
  it('a page left untouched never refreshes itself, even long past the TTL', async () => {
    const wrapper = await mountAt()
    const afterMount = spendingReads()
    expect(afterMount).toBeGreaterThan(0)

    // A full minute of sitting still: no focus, no navigation, no click.
    await advance(TTL_MS * 4)

    // The TTL expiring is not an event. Nothing may be invented here.
    expect(spendingReads()).toBe(afterMount)
    wrapper.unmount()
  })

  it('window focus after the TTL expires picks up an external (CLI) write', async () => {
    const wrapper = await mountAt()
    const afterMount = spendingReads()

    // The CLI adds a subscription to the same SQLite file.
    const updatedRow = {
      currency: 'CNY',
      monthlyEquivalentMinor: 16800,
      annualEquivalentMinor: 201600,
      subscriptionCount: 2,
      hybridCount: 0,
    }
    apiMock.spendingOverview.mockImplementation(
      async () =>
        fixtures.spending({
          byCurrency: [updatedRow],
          counts: { total: 2, active: 2, paused: 0, cancelled: 0, expired: 0, unknown: 0 },
        }) as never,
    )

    await advance(TTL_MS + 1000)
    expect(spendingReads()).toBe(afterMount) // still idle: no event yet

    window.dispatchEvent(new Event('focus'))
    await flushPromises()

    expect(spendingReads()).toBe(afterMount + 1)
    expect(wrapper.text()).toContain('168.00 CNY')
    wrapper.unmount()
  })

  it('window focus inside the TTL is a cache hit, not an extra read', async () => {
    const wrapper = await mountAt()
    const afterMount = spendingReads()

    await advance(1000)
    window.dispatchEvent(new Event('focus'))
    await flushPromises()

    expect(spendingReads()).toBe(afterMount)
    wrapper.unmount()
  })

  it('re-entering the page after the TTL expires refetches', async () => {
    const first = await mountAt()
    const afterMount = spendingReads()
    first.unmount()

    await advance(TTL_MS + 1000)
    const second = await mountAt()

    expect(spendingReads()).toBeGreaterThan(afterMount)
    expect(second.text()).toContain('成本概览')
    second.unmount()
  })

  it('the window focus listener is removed on unmount', async () => {
    const wrapper = await mountAt()
    wrapper.unmount()
    const afterUnmount = spendingReads()

    await advance(TTL_MS + 1000)
    window.dispatchEvent(new Event('focus'))
    await flushPromises()

    expect(spendingReads()).toBe(afterUnmount)
  })

  it("the GUI's own write updates immediately, without waiting for the TTL", async () => {
    const wrapper = await mountAt()
    const afterMount = spendingReads()

    // Refresh detections is the simplest write path on this page.
    const refreshButton = wrapper.findAll('button').find((button) => button.text().includes('刷新'))!
    await refreshButton.trigger('click')
    await flushPromises()

    // Well inside the TTL: only invalidate-then-refetch can explain this read.
    expect(spendingReads()).toBeGreaterThan(afterMount)
    wrapper.unmount()
  })
})

describe('M1-C.1 refresh contract: the copy matches the mechanism', () => {
  const zh = i18n.global.t('stack.spendingEmpty', {}, { locale: 'zh' }) as string
  const en = i18n.global.t('stack.spendingEmpty', {}, { locale: 'en' }) as string

  it('never promises a periodic automatic refresh that no timer implements', () => {
    expect(zh).not.toContain('自动刷新')
    expect(zh).not.toMatch(/每\s*15\s*秒/)
    expect(en).not.toMatch(/automatically every 15 seconds/i)
    expect(en).not.toMatch(/refreshes automatically/i)
  })

  it('states the two things that really do refresh: own writes and re-activation', () => {
    // zh: own changes sync immediately; other programs' changes land on reopen/refocus
    expect(zh).toContain('立即同步')
    expect(zh).toMatch(/重新打开本页|切回窗口/)

    // en: same two facts
    expect(en.toLowerCase()).toContain('sync immediately')
    expect(en.toLowerCase()).toMatch(/reopen this page|switch back to the window/)
  })
})
