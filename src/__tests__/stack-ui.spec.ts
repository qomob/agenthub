import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import StackDetailView from '../pages/StackDetailView.vue'
import StackView from '../pages/StackView.vue'
import { i18n } from '../i18n'
import { router } from '../router'
import { useStackStore } from '../stores/stack'
import type { SpendingOverview, StackItemDetail, StackItemView, SubscriptionView } from '../lib/types'

// UX-5: pure fixtures only — nothing here reads the real ~/.agenthub or any
// Agent directory; the api layer is mocked entirely.
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

  const subscription = (overrides: Record<string, unknown> = {}): Record<string, unknown> => ({
    id: 'sub_1',
    stackItemId: 'st_item',
    productName: 'Codex Pro',
    billingType: 'subscription',
    amountMinor: 2000,
    currency: 'USD',
    billingPeriod: 'monthly',
    status: 'active',
    renewsOn: '2026-11-15',
    createdAt: '2026-11-11T00:00:00Z',
    updatedAt: '2026-11-11T00:00:00Z',
    ...overrides,
  })

  const unknownSubscription: Record<string, unknown> = {
    id: 'sub_2',
    stackItemId: 'st_item',
    productName: 'Codex Usage',
    billingType: 'usage',
    billingPeriod: 'usage',
    status: 'active',
    createdAt: '2026-11-11T00:00:00Z',
    updatedAt: '2026-11-11T00:00:00Z',
  }

  const spending = (overrides: Record<string, unknown> = {}): Record<string, unknown> => ({
    byCurrency: [
      {
        currency: 'USD',
        monthlyEquivalentMinor: 3000,
        annualEquivalentMinor: 36000,
        subscriptionCount: 2,
        hybridCount: 0,
      },
    ],
    upcomingRenewals: [subscription()],
    renewalsUnknown: [],
    staleRenewals: [],
    laterRenewals: [],
    incomplete: [{ subscription: unknownSubscription, missing: ['amount', 'currency'] }],
    usageWithoutBills: [unknownSubscription],
    counts: { total: 2, active: 2, paused: 0, cancelled: 0, expired: 0, unknown: 0 },
    generatedAt: '2026-11-11T00:00:00Z',
    upcomingWindowDays: 7,
    ...overrides,
  })

  const stackGetDetail = (): Record<string, unknown> => ({
    item: item(),
    detections: [
      {
        id: 'det_1',
        stackItemId: 'st_item',
        adapterKey: 'codex',
        detectedStatus: 'installed',
        evidence: ['~/.codex/config.toml exists'],
        detectedAt: '2026-11-11T00:00:00Z',
      },
    ],
    subscriptions: [subscription(), unknownSubscription],
    mergedChildren: [],
  })

  const apiMock = {
    stackList: vi.fn(async () => [
      item(),
      item({ id: 'st_other', name: 'Trae', subscriptionState: 'none', subscriptionCount: 0 }),
    ]),
    stackGet: vi.fn(async () => stackGetDetail()),
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
    subscriptionUpsert: vi.fn(async () => subscription()),
    subscriptionDelete: vi.fn(async () => undefined),
    spendingOverview: vi.fn(async () => spending()),
  }

  return { item, subscription, spending, stackGetDetail, apiMock }
})

type apiShape = {
  stackList: ReturnType<typeof vi.fn>
  stackGet: ReturnType<typeof vi.fn>
  stackUpsertItem: ReturnType<typeof vi.fn>
  stackConfirmItem: ReturnType<typeof vi.fn>
  stackMergeItems: ReturnType<typeof vi.fn>
  stackUnmergeItem: ReturnType<typeof vi.fn>
  stackDeleteItem: ReturnType<typeof vi.fn>
  stackRefreshDetections: ReturnType<typeof vi.fn>
  subscriptionUpsert: ReturnType<typeof vi.fn>
  subscriptionDelete: ReturnType<typeof vi.fn>
  spendingOverview: ReturnType<typeof vi.fn>
}

const item = fixtures.item as unknown as (overrides?: Partial<StackItemView>) => StackItemView
const subscription = fixtures.subscription as unknown as (
  overrides?: Partial<SubscriptionView>,
) => SubscriptionView
const spending = fixtures.spending as unknown as (
  overrides?: Partial<SpendingOverview>,
) => SpendingOverview
const stackGetDetail = fixtures.stackGetDetail as unknown as () => StackItemDetail
const apiMock = fixtures.apiMock as unknown as apiShape

/** UX-1 default: aggregate overflow fails the spending call while items stay fine. */
function mockSpendingOverflow(): void {
  apiMock.spendingOverview.mockReset()
  apiMock.spendingOverview.mockImplementation(async () => {
    throw Object.assign(new Error('Spending aggregation exceeded the safe integer range'), {
      code: 'INVALID_INPUT',
    })
  })
}

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

function mountPage(component: typeof StackView | typeof StackDetailView): ReturnType<typeof mount> {
  return mount(component, {
    global: { plugins: [createPinia(), router, i18n], stubs: { teleport: true } },
    attachTo: document.body,
  })
}

/**
 * UX-1 invariant: a zero-cost amount must never appear as a fabricated "0.00"
 * while spending data is missing/failed. Extracted so the reverse test can
 * prove this assertion actually goes red when the UI regresses.
 */
function assertNoFakeZero(pageText: string): void {
  // (?<!\d): match a true "0.00", not the tail of "20.00"/"30.00".
  expect(pageText).not.toMatch(/(?<!\d)0\.00 USD/)
}

/** Restore the default happy-path implementations between tests. */
function restoreDefaults(): void {
  apiMock.stackList.mockReset()
  apiMock.stackList.mockImplementation(async () => [
    item(),
    item({ id: 'st_other', name: 'Trae', subscriptionState: 'none', subscriptionCount: 0 }),
  ])
  apiMock.stackGet.mockReset()
  apiMock.stackGet.mockImplementation(async () => stackGetDetail())
  apiMock.spendingOverview.mockReset()
  apiMock.spendingOverview.mockImplementation(async () => spending())
}

describe('My AI Stack UI', () => {
  beforeEach(restoreDefaults)
  it('renders the item list with detection/source/subscription badges (UX-2 explicit facts)', async () => {
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    const text = wrapper.text()
    expect(text).toContain('Codex')
    expect(text).toContain('已安装')
    expect(text).toContain('检测到')
    expect(text).toContain('已登记订阅')
    expect(text).toContain('未确认')
    // spending overview renders per-currency without summing currencies
    expect(text).toContain('成本概览')
    expect(text).toContain('30.00 USD')
    expect(text).toContain('月度等效成本是折算值')
    wrapper.unmount()
  })

  it('UX-1: keeps items visible and shows the failure banner when spending overflows — never a fake zero', async () => {
    mockSpendingOverflow()
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    const text = wrapper.text()
    expect(text).toContain('成本计算失败')
    expect(text).toContain('不是数据丢失')
    // the item list must still be visible
    expect(text).toContain('Codex')
    // no zero-cost fabrication (reverse test below proves this can go red)
    assertNoFakeZero(text)
    wrapper.unmount()
  })

  it('UX-1 reverse: the fake-zero assertion itself fails when the UI regresses to a fabricated 0.00', () => {
    // Simulate a regressed UI: spending failed yet the page still renders a
    // fabricated zero (e.g. a broken formatter printing 0 for unknown money).
    // The invariant helper must throw for this input — proving the UX-1
    // assertion is load-bearing, not vacuously true.
    const regressedUiText = [
      'Codex',
      '成本计算失败', // failure banner IS shown (like the real UX-1 page)
      '0.00 USD', // ...but a fabricated zero leaked into the cost column
    ].join('\n')
    expect(() => assertNoFakeZero(regressedUiText)).toThrow()
    // and the healthy page text (no zeros anywhere) still passes
    expect(() => assertNoFakeZero('Codex\n成本计算失败\n30.00 USD')).not.toThrow()
  })

  it('UX-2: shows "amount unknown" chips for incomplete subscriptions instead of zeros', async () => {
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    const text = wrapper.text()
    expect(text).toContain('信息不完整')
    expect(text).toContain('缺少金额')
    expect(text).toContain('缺少币种')
    expect(text).toContain('按量计费且无账单数据，无法折算')
    wrapper.unmount()
  })

  it('opens the manual add editor and validates the required name', async () => {
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    await wrapper.findAll('button').find((button) => button.text().includes('手动添加工具'))!.trigger('click')
    await flushPromises()
    await wrapper.find('#stack-name').setValue('')
    // save without a name shows the validation error and does not call the API
    const saveButton = wrapper.findAll('button').find((button) => button.text().includes('保存'))!
    await saveButton.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('名称必填')
    expect(apiMock.stackUpsertItem).not.toHaveBeenCalled()
    // a real save reaches the API with the typed name
    await wrapper.find('#stack-name').setValue('Cursor')
    await saveButton.trigger('click')
    await flushPromises()
    expect(apiMock.stackUpsertItem).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'Cursor', id: undefined }),
    )
    wrapper.unmount()
  })

  it('UX-3: refresh keeps the list attached and merge filtering excludes already-merged targets', async () => {
    apiMock.stackList.mockResolvedValue([
      item(),
      item({ id: 'st_other', name: 'Trae', subscriptionState: 'none', subscriptionCount: 0 }),
      item({ id: 'st_merged', name: 'Old Tool', mergedInto: 'st_item' }),
    ])
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    const text = wrapper.text()
    expect(text).toContain('已并入')
    // open merge for the unmerged item: merged item must not appear as a target
    const mergeButton = wrapper.findAll('button').find((button) => button.text().includes('合并'))
    expect(mergeButton).toBeTruthy()
    await mergeButton!.trigger('click')
    await flushPromises()
    const modal = wrapper.text()
    expect(modal).toContain('选择要并入的目标条目')
    const candidateIds = wrapper
      .findAll('input[type="radio"]')
      .map((radio) => (radio.element as HTMLInputElement).value)
    expect(candidateIds).toEqual(['st_other'])
    // complete the flow: pick the target and send it through the API
    await wrapper.find('input[type="radio"]').setValue(true)
    const entryButton = wrapper
      .findAll('button')
      .filter((button) => button.text() === '并入其他条目')
      .at(-1)
    await entryButton!.trigger('click')
    await flushPromises()
    expect(apiMock.stackMergeItems).toHaveBeenCalledWith('st_item', 'st_other')
    wrapper.unmount()
  })


  // -- M1-C.1 permanent regressions (Reproductions A/B from M1_C_ACCEPTANCE_REPORT §5) --

  it('M1-C.1 Reproduction A: re-entering the page after the TTL expires picks up an external (CLI) write', async () => {
    vi.useFakeTimers()
    try {
      await router.push('/stack')
      await router.isReady()
      // One shared pinia so a remount simulates navigating back into the page.
      const pinia = createPinia()
      setActivePinia(pinia)
      const mountWith = () =>
        mount(StackView, {
          global: { plugins: [pinia, router, i18n], stubs: { teleport: true } },
          attachTo: document.body,
        })
      const wrapper = mountWith()
      await flushPromises()
      expect(apiMock.spendingOverview).toHaveBeenCalledTimes(1)

      // CLI adds a subscription behind the GUI's back.
      apiMock.spendingOverview.mockImplementation(async () => spending({ counts: { total: 5, active: 5, paused: 0, cancelled: 0, expired: 0, unknown: 0 } }))
      apiMock.stackList.mockImplementation(async () => [item({ subscriptionCount: 2 })])

      // User navigates away and re-enters after the 15s TTL.
      vi.advanceTimersByTime(15_001)
      wrapper.unmount()
      const wrapper2 = mountWith()
      await flushPromises()
      expect(apiMock.spendingOverview).toHaveBeenCalledTimes(2)
      expect(useStackStore().spending?.counts.total).toBe(5)
      wrapper2.unmount()
    } finally {
      vi.useRealTimers()
    }
  })

  it('M1-C.1 Reproduction B: deleting a subscription in the GUI invalidates the spending cache immediately', async () => {
    await router.push('/stack/st_item')
    await router.isReady()
    const wrapper = mountPage(StackDetailView)
    await flushPromises()
    const store = useStackStore()
    const callsBefore = apiMock.spendingOverview.mock.calls.length

    // Delete the subscription via the confirm-gated dialog. The subscription
    // row button is a small ghost button; the page-level item delete lives in
    // the header, so target the subscription row explicitly.
    const deleteButton = wrapper
      .findAll('button')
      .filter((button) => button.text().includes('删除'))
      .find((button) => button.classes().includes('btn-ghost'))
    expect(deleteButton).toBeTruthy()
    await deleteButton!.trigger('click')
    await flushPromises()
    const confirmButton = wrapper.find('.btn-danger-solid')
    expect(confirmButton.exists()).toBe(true)
    // What core would return after the delete (4 subscriptions registered).
    apiMock.spendingOverview.mockImplementation(async () => spending({ counts: { total: 4, active: 4, paused: 0, cancelled: 0, expired: 0, unknown: 0 } }))
    await confirmButton.trigger('click')
    await flushPromises()
    expect(apiMock.subscriptionDelete).toHaveBeenCalledWith('sub_1')

    // The cache must have been invalidated by the write path: the overview is
    // refetched from core (not served from the old cache) and reflects the
    // database truth that exists AFTER the delete.
    await flushPromises()
    expect(apiMock.spendingOverview.mock.calls.length).toBeGreaterThan(callsBefore)
    const overview = store.spending
    expect(overview?.counts.total).toBe(4)
    wrapper.unmount()
  })

  it('renders the guided EmptyState for an empty item list', async () => {
    apiMock.stackList.mockResolvedValue([])
    apiMock.spendingOverview.mockResolvedValue(
      spending({
        byCurrency: [],
        counts: { total: 0, active: 0, paused: 0, cancelled: 0, expired: 0, unknown: 0 },
      }),
    )
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    const text = wrapper.text()
    expect(text).toContain('还没有登记任何工具')
    expect(text).toContain('先运行「检测刷新」')
    wrapper.unmount()
  })

  it('renders one row per currency without merging sums across currencies', async () => {
    apiMock.spendingOverview.mockResolvedValue(
      spending({
        byCurrency: [
          {
            currency: 'USD',
            monthlyEquivalentMinor: 3000,
            annualEquivalentMinor: 36000,
            subscriptionCount: 2,
            hybridCount: 0,
          },
          {
            currency: 'CNY',
            monthlyEquivalentMinor: 6800,
            annualEquivalentMinor: 81600,
            subscriptionCount: 1,
            hybridCount: 0,
          },
        ],
      }),
    )
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    const text = wrapper.text()
    expect(text).toContain('30.00 USD')
    expect(text).toContain('360.00 USD')
    expect(text).toContain('68.00 CNY')
    expect(text).toContain('816.00 CNY')
    // one row per currency, never a summed total
    expect(wrapper.findAll('tbody tr')).toHaveLength(2)
    expect(text).not.toContain('98.00')
    wrapper.unmount()
  })

  it('blocks item deletion behind an explicit confirm dialog (UX-3 delete confirmation)', async () => {
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    expect(apiMock.stackDeleteItem).not.toHaveBeenCalled()
    const deleteButton = wrapper.findAll('button').find((button) => button.text() === '删除')
    expect(deleteButton).toBeTruthy()
    await deleteButton!.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('删除工具')
    expect(wrapper.text()).toContain('此操作无法撤销')
    expect(apiMock.stackDeleteItem).not.toHaveBeenCalled()
    const confirmButton = wrapper.find('.btn-danger-solid')
    expect(confirmButton.exists()).toBe(true)
    await confirmButton.trigger('click')
    await flushPromises()
    expect(apiMock.stackDeleteItem).toHaveBeenCalledWith('st_item')
    wrapper.unmount()
  })
})

describe('My AI Stack detail UI', () => {
  beforeEach(restoreDefaults)
  it('shows detections, subscriptions and explicit unknown amounts (UX-2)', async () => {
    await router.push('/stack/st_item')
    await router.isReady()
    const wrapper = mountPage(StackDetailView)
    await flushPromises()
    const text = wrapper.text()
    expect(text).toContain('Codex')
    expect(text).toContain('检测记录')
    expect(text).toContain('~/.codex/config.toml exists')
    expect(text).toContain('Codex Pro')
    expect(text).toContain('20.00 USD')
    // usage subscription without amount shows the explicit unknown label
    expect(text).toContain('金额未知')
    expect(text).not.toMatch(/(?<!\d)0\.00 USD/)
    wrapper.unmount()
  })

  it('validates the subscription form: currency required with amount, product required', async () => {
    await router.push('/stack/st_item')
    await router.isReady()
    const wrapper = mountPage(StackDetailView)
    await flushPromises()
    // open the add-subscription modal
    const addSub = wrapper.findAll('button').find((button) => button.text().includes('添加订阅'))!
    await addSub.trigger('click')
    await flushPromises()
    // amount without currency
    await wrapper.find('#sub-product').setValue('Codex Pro')
    await wrapper.find('#sub-amount').setValue('20')
    const save = wrapper.findAll('button').find((button) => button.text().includes('保存'))!
    await save.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('填写金额时必须填写币种')
    expect(apiMock.subscriptionUpsert).not.toHaveBeenCalled()
    // valid save reaches the API with integer minor units
    await wrapper.find('#sub-currency').setValue('CNY')
    await save.trigger('click')
    await flushPromises()
    expect(apiMock.subscriptionUpsert).toHaveBeenCalledWith(
      expect.objectContaining({
        productName: 'Codex Pro',
        amountMinor: 2000,
        currency: 'CNY',
        stackItemId: 'st_item',
      }),
    )
    wrapper.unmount()
  })

  it('UX-3: unmerge is confirm-gated and detail keeps the back link', async () => {
    apiMock.stackGet.mockResolvedValue({
      ...stackGetDetail(),
      item: item({ mergedInto: 'st_parent', name: 'Old Tool' }),
    })
    await router.push('/stack/st_item')
    await router.isReady()
    const wrapper = mountPage(StackDetailView)
    await flushPromises()
    expect(wrapper.text()).toContain('返回 AI Stack')
    expect(wrapper.text()).toContain('已并入')
    const unmergeButton = wrapper.findAll('button').find((button) => button.text().includes('撤销并入'))!
    await unmergeButton.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('撤销「Old Tool」的并入操作')
    expect(apiMock.stackUnmergeItem).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('merge flow: picking a target on the detail page confirms, then calls stackMergeItems(child, into)', async () => {
    apiMock.stackList.mockResolvedValue([
      item(),
      item({ id: 'st_other', name: 'Trae', subscriptionState: 'none', subscriptionCount: 0 }),
      item({ id: 'st_merged', name: 'Old Tool', mergedInto: 'st_item' }),
    ])
    await router.push('/stack/st_item')
    await router.isReady()
    const wrapper = mountPage(StackDetailView)
    await flushPromises()
    const mergeButton = wrapper
      .findAll('button')
      .find((button) => button.text() === '并入其他条目')
    expect(mergeButton).toBeTruthy()
    await mergeButton!.trigger('click')
    await flushPromises()
    // already-merged items and self are excluded from the target picker
    const candidateIds = wrapper
      .findAll('input[type="radio"]')
      .map((radio) => (radio.element as HTMLInputElement).value)
    expect(candidateIds).toEqual(['st_other'])
    await wrapper.find('input[type="radio"]').setValue(true)
    const entryButton = wrapper
      .findAll('button')
      .filter((button) => button.text() === '并入其他条目')
      .at(-1)
    await entryButton!.trigger('click')
    await flushPromises()
    // the explicit ConfirmDialog names both sides before anything is sent
    expect(wrapper.text()).toContain('将「Codex」并入「Trae」')
    expect(wrapper.text()).toContain('订阅数据不会移动，可随时撤销')
    expect(apiMock.stackMergeItems).not.toHaveBeenCalled()
    const dialogConfirm = wrapper.findAll('.btn-primary').at(-1)
    await dialogConfirm!.trigger('click')
    await flushPromises()
    expect(apiMock.stackMergeItems).toHaveBeenCalledWith('st_item', 'st_other')
    wrapper.unmount()
  })
})

describe('M1-C.1 write-path cache invalidation', () => {
  beforeEach(restoreDefaults)

  /** Number of spending reads so far — used to prove a write refetched core. */
  const spendingCalls = () => apiMock.spendingOverview.mock.calls.length

  it('adding a subscription refetches the overview immediately (no TTL wait)', async () => {
    await router.push('/stack/st_item')
    await router.isReady()
    const wrapper = mountPage(StackDetailView)
    await flushPromises()
    const before = spendingCalls()

    const addSub = wrapper.findAll('button').find((button) => button.text().includes('添加订阅'))!
    await addSub.trigger('click')
    await flushPromises()
    await wrapper.find('#sub-product').setValue('Cursor Pro')
    await wrapper.find('#sub-amount').setValue('20')
    await wrapper.find('#sub-currency').setValue('USD')
    const save = wrapper.findAll('button').find((button) => button.text().includes('保存'))!
    await save.trigger('click')
    await flushPromises()

    expect(apiMock.subscriptionUpsert).toHaveBeenCalled()
    expect(spendingCalls()).toBeGreaterThan(before)
    expect(useStackStore().spendingLoadedAt).toBeGreaterThan(0)
    wrapper.unmount()
  })

  it('editing a subscription amount refetches the overview', async () => {
    await router.push('/stack/st_item')
    await router.isReady()
    const wrapper = mountPage(StackDetailView)
    await flushPromises()
    const before = spendingCalls()

    // the subscription-row editor is a btn-ghost button (the header item edit
    // is a plain .btn), so target the row one explicitly
    const editButton = wrapper
      .findAll('button')
      .filter((button) => button.text().includes('编辑'))
      .find((button) => button.classes().includes('btn-ghost'))!
    await editButton.trigger('click')
    await flushPromises()
    await wrapper.find('#sub-amount').setValue('35')
    const save = wrapper.findAll('button').find((button) => button.text().includes('保存'))!
    await save.trigger('click')
    await flushPromises()

    expect(apiMock.subscriptionUpsert).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'sub_1', amountMinor: 3500 }),
    )
    expect(spendingCalls()).toBeGreaterThan(before)
    wrapper.unmount()
  })

  it('merging on the list page refetches the overview', async () => {
    apiMock.stackList.mockResolvedValue([
      item(),
      item({ id: 'st_other', name: 'Trae', subscriptionState: 'none', subscriptionCount: 0 }),
    ])
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    const before = spendingCalls()

    const mergeButton = wrapper.findAll('button').find((button) => button.text().includes('合并'))!
    await mergeButton.trigger('click')
    await flushPromises()
    await wrapper.find('input[type="radio"]').setValue(true)
    const entryButton = wrapper
      .findAll('button')
      .filter((button) => button.text() === '并入其他条目')
      .at(-1)
    await entryButton!.trigger('click')
    await flushPromises()

    expect(apiMock.stackMergeItems).toHaveBeenCalledWith('st_item', 'st_other')
    expect(spendingCalls()).toBeGreaterThan(before)
    wrapper.unmount()
  })

  it('undoing a merge on the list page refetches the overview', async () => {
    apiMock.stackList.mockResolvedValue([
      item({ mergedInto: 'st_parent' }),
      item({ id: 'st_other', name: 'Trae', subscriptionState: 'none', subscriptionCount: 0 }),
    ])
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    const before = spendingCalls()

    const undoButton = wrapper.findAll('button').find((button) => button.text().includes('撤销并入'))!
    await undoButton.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('撤销「Codex」的并入操作')
    // unmerge is reversible, so its confirm is a primary (not danger) action;
    // the dialog footer confirm is the LAST primary button on the page
    const confirmButton = wrapper.findAll('.btn-primary').at(-1)
    expect(confirmButton).toBeTruthy()
    await confirmButton!.trigger('click')
    await flushPromises()

    expect(apiMock.stackUnmergeItem).toHaveBeenCalledWith('st_item')
    expect(spendingCalls()).toBeGreaterThan(before)
    wrapper.unmount()
  })

  it('editing a tool item refetches the overview', async () => {
    await router.push('/stack/st_item')
    await router.isReady()
    const wrapper = mountPage(StackDetailView)
    await flushPromises()
    const before = spendingCalls()

    // the item-level 编辑 lives in the header, above the subscription rows
    const editButton = wrapper.findAll('button').filter((button) => button.text().includes('编辑')).at(0)!
    await editButton.trigger('click')
    await flushPromises()
    await wrapper.find('#detail-item-name').setValue('Codex Renamed')
    const save = wrapper.findAll('button').find((button) => button.text().includes('保存'))!
    await save.trigger('click')
    await flushPromises()

    expect(apiMock.stackUpsertItem).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'st_item', name: 'Codex Renamed' }),
    )
    expect(spendingCalls()).toBeGreaterThan(before)
    wrapper.unmount()
  })

  it('cancelling a delete neither writes nor refetches', async () => {
    apiMock.stackList.mockResolvedValue([
      item(),
      item({ id: 'st_other', name: 'Trae', subscriptionState: 'none', subscriptionCount: 0 }),
    ])
    await router.push('/stack')
    await router.isReady()
    const wrapper = mountPage(StackView)
    await flushPromises()
    const before = spendingCalls()

    const deleteButton = wrapper.findAll('button').find((button) => button.text() === '删除')!
    await deleteButton.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('此操作无法撤销')
    const cancelButton = wrapper.findAll('button').find((button) => button.text() === '取消')!
    await cancelButton.trigger('click')
    await flushPromises()

    expect(apiMock.stackDeleteItem).not.toHaveBeenCalled()
    expect(spendingCalls()).toBe(before)
    wrapper.unmount()
  })
})
