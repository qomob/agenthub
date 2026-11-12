import { flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { useStackStore } from '../stores/stack'
import type { SpendingOverview, StackItemView } from '../lib/types'

// M1-C.1 regression suite for the Spending Overview cache lifecycle.
// Reproductions A/B from docs/proposals/M1_C_ACCEPTANCE_REPORT.md §5 live here
// as permanent automated tests. Pure fixtures — the api layer is fully mocked.

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
    subscriptionCount: 1,
    ...overrides,
  })

  const spending = (registered: number, usdMonthly = 2750): Record<string, unknown> => ({
    byCurrency: [
      {
        currency: 'USD',
        monthlyEquivalentMinor: usdMonthly,
        annualEquivalentMinor: usdMonthly * 12,
        subscriptionCount: registered,
        hybridCount: 0,
      },
    ],
    upcomingRenewals: [],
    renewalsUnknown: [],
    staleRenewals: [],
    laterRenewals: [],
    incomplete: [],
    usageWithoutBills: [],
    counts: {
      total: registered,
      active: registered,
      paused: 0,
      cancelled: 0,
      expired: 0,
      unknown: 0,
    },
    generatedAt: '2026-11-11T00:00:00Z',
    upcomingWindowDays: 7,
  })

  const apiMock = {
    stackList: vi.fn(),
    stackGet: vi.fn(),
    stackUpsertItem: vi.fn(),
    stackConfirmItem: vi.fn(),
    stackMergeItems: vi.fn(),
    stackUnmergeItem: vi.fn(),
    stackDeleteItem: vi.fn(),
    stackRefreshDetections: vi.fn(),
    subscriptionUpsert: vi.fn(),
    subscriptionDelete: vi.fn(),
    spendingOverview: vi.fn(),
  }

  return { item, spending, apiMock }
})

const item = fixtures.item as unknown as (overrides?: Partial<StackItemView>) => StackItemView
const spending = fixtures.spending as unknown as (registered: number) => SpendingOverview
const apiMock = fixtures.apiMock

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

beforeEach(() => {
  setActivePinia(createPinia())
  vi.useFakeTimers()
  apiMock.stackList.mockReset()
  apiMock.stackList.mockImplementation(async () => [item()])
  apiMock.spendingOverview.mockReset()
  apiMock.spendingOverview.mockImplementation(async () => spending(4))
})

afterEach(() => {
  vi.useRealTimers()
})

describe('stack store: spending cache lifecycle (M1-C.1)', () => {
  it('first load fetches spending from core', async () => {
    const store = useStackStore()
    await store.loadSpending()
    expect(apiMock.spendingOverview).toHaveBeenCalledTimes(1)
    expect(store.spending?.counts.total).toBe(4)
  })

  it('repeated reads within the TTL reuse the cache without new requests', async () => {
    const store = useStackStore()
    await store.loadSpending()
    await store.loadSpending()
    await store.loadSpending()
    expect(apiMock.spendingOverview).toHaveBeenCalledTimes(1)
  })

  it('TTL expiry refetches — a CLI write outside the GUI is picked up on re-entry (Reproduction A)', async () => {
    const store = useStackStore()
    await store.loadSpending()
    expect(store.spending?.counts.total).toBe(4)

    // External writer (CLI) changes the database while the GUI sits idle.
    apiMock.spendingOverview.mockImplementation(async () => spending(5))

    // Re-enter the page after the 15s TTL has expired.
    vi.advanceTimersByTime(15_001)
    await store.loadSpending()
    expect(apiMock.spendingOverview).toHaveBeenCalledTimes(2)
    expect(store.spending?.counts.total).toBe(5)
  })

  it('invalidateSpending() forces the next read to refetch (Reproduction B: GUI delete path)', async () => {
    const store = useStackStore()
    await store.loadSpending()
    expect(store.spending?.counts.total).toBe(4)

    // The GUI's own successful write must not leave stale figures behind.
    apiMock.spendingOverview.mockImplementation(async () => spending(3))
    store.invalidateSpending()
    // Even within the TTL window the next read must hit core again.
    await store.loadSpending()
    expect(apiMock.spendingOverview).toHaveBeenCalledTimes(2)
    expect(store.spending?.counts.total).toBe(3)
  })

  it('concurrent reads share one in-flight request', async () => {
    let resolveSpending!: (value: SpendingOverview) => void
    apiMock.spendingOverview.mockImplementation(
      () => new Promise<SpendingOverview>((resolve) => (resolveSpending = resolve)),
    )
    const store = useStackStore()
    const p1 = store.loadSpending()
    const p2 = store.loadSpending()
    resolveSpending(spending(4))
    await Promise.all([p1, p2])
    expect(apiMock.spendingOverview).toHaveBeenCalledTimes(1)
  })

  it('a stale response that resolves after invalidateSpending() must not repopulate the cache', async () => {
    const resolvers: Array<(value: SpendingOverview) => void> = []
    apiMock.spendingOverview.mockImplementation(
      () =>
        new Promise<SpendingOverview>(
          (resolve) => resolvers.push(resolve) && undefined,
        ),
    )
    const store = useStackStore()
    const stale = store.loadSpending() // request #1 (old)
    await flushPromises()
    store.invalidateSpending() // GUI write succeeds; cache invalidated
    const fresh = store.loadSpending() // request #2 (new)
    // The old response arrives LAST and must be discarded.
    resolvers[0](spending(99))
    resolvers[1](spending(3))
    await Promise.all([stale, fresh])
    expect(store.spending?.counts.total).toBe(3)
  })

  it('a refresh failure keeps the last good figures and surfaces the error — never a fabricated zero', async () => {
    const store = useStackStore()
    await store.loadSpending()
    apiMock.spendingOverview.mockImplementation(async () => {
      throw Object.assign(new Error('annual_equivalent_total exceeds safe integer bound'), {
        code: 'INVALID_INPUT',
      })
    })
    vi.advanceTimersByTime(15_001)
    const result = await store.loadSpending()
    expect(result).toBeNull()
    expect(store.spendingError).toContain('annual_equivalent_total')
    // Last good data is retained (not reset to zero/null) for the UI.
    expect(store.spending?.counts.total).toBe(4)
  })

  it('recovery after failure: the next loadSpending retries automatically', async () => {
    const store = useStackStore()
    apiMock.spendingOverview.mockImplementationOnce(async () => {
      throw new Error('overflow')
    })
    await store.loadSpending()
    expect(store.spendingError).not.toBeNull()
    apiMock.spendingOverview.mockImplementation(async () => spending(4))
    await store.loadSpending()
    expect(store.spending).not.toBeNull()
    expect(store.spendingError).toBeNull()
  })
})
