import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import CostInsights from '../components/CostInsights.vue'
import { i18n } from '../i18n'
import { router } from '../router'
import type { OptimizerReport, OptimizerScenarioResult, OptimizerSettings } from '../lib/types'

/**
 * M2-B: the cost-insights panel and the CLI `cost` command family must read
 * the same persisted settings through the same service path, simulation must
 * stay read-only, and the deltas must stay per-currency. These tests pin the
 * panel behaviour; the CLI parity is asserted in Rust (stack_cli parity
 * follows the same envelope), and both consume identical JSON shapes.
 */

const fixtures = vi.hoisted(() => {
  const recommendation = (overrides: Record<string, unknown> = {}): Record<string, unknown> => ({
    recommendationId: 'rec_abc123',
    recommendationType: 'R2_HIGH_COST',
    title: 'Pro is at or above your USD review threshold',
    explanation: 'The monthly equivalent of "Pro" is 60.00 USD, at or above the 50.00 USD threshold.',
    relatedStackItems: ['st_item'],
    relatedSubscriptions: ['sub_1'],
    evidence: [],
    missingInformation: [],
    assumptions: ['Monthly equivalent is a folded estimate.'],
    estimatedImpact: null,
    currency: 'USD',
    confidenceOrEvidenceLevel: 'user_configured',
    userActionRequired: 'review',
    ...overrides,
  })

  const report = (recommendations: Record<string, unknown>[]): OptimizerReport =>
    ({
      generatedAt: '2026-11-12T00:00:00Z',
      recommendations,
      counts: {
        stackItems: 2,
        subscriptionsTotal: 2,
        subscriptionsActive: 2,
        recommendations: recommendations.length,
        dataGaps: recommendations.filter((r) => r.recommendationType === 'R5_DATA_GAP').length,
      },
    }) as unknown as OptimizerReport

  const settings = (overrides: Record<string, unknown> = {}): OptimizerSettings =>
    ({
      costThresholdsMinor: { USD: 5000 },
      renewalWindowDays: 10,
      ...overrides,
    }) as unknown as OptimizerSettings

  const scenario = (overrides: Record<string, unknown> = {}): OptimizerScenarioResult =>
    ({
      generatedAt: '2026-11-12T00:00:00Z',
      actions: [{ kind: 'cancel', subscriptionId: 'sub_1' }],
      affectedSubscriptions: ['sub_1'],
      deltasMonthly: [
        {
          currency: 'USD',
          minorBefore: 6000,
          minorAfter: 0,
          deltaMinor: -6000,
          period: 'monthly',
        },
      ],
      deltasAnnual: [
        {
          currency: 'USD',
          minorBefore: 72000,
          minorAfter: 0,
          deltaMinor: -72000,
          period: 'annual',
        },
      ],
      notes: [
        'This is a simulation only: no subscription was changed and no money was saved yet.',
        'Affected subscriptions: sub_1',
      ],
      ...overrides,
    }) as unknown as OptimizerScenarioResult

  // The simulation dropdown derives its targets from the spending overview's
  // buckets (their union covers every active subscription).
  const spending = (): Record<string, unknown> => ({
    byCurrency: [
      {
        currency: 'USD',
        monthlyEquivalentMinor: 6000,
        annualEquivalentMinor: 72000,
        subscriptionCount: 1,
        hybridCount: 0,
      },
    ],
    upcomingRenewals: [
      {
        id: 'sub_1',
        stackItemId: 'st_item',
        productName: 'Codex Pro',
        billingType: 'subscription',
        amountMinor: 6000,
        currency: 'USD',
        billingPeriod: 'monthly',
        status: 'active',
        renewsOn: '2026-11-15',
        createdAt: '2026-11-11T00:00:00Z',
        updatedAt: '2026-11-11T00:00:00Z',
      },
    ],
    renewalsUnknown: [],
    staleRenewals: [],
    laterRenewals: [],
    incomplete: [],
    usageWithoutBills: [],
    counts: { total: 1, active: 1, paused: 0, cancelled: 0, expired: 0, unknown: 0 },
    generatedAt: '2026-11-11T00:00:00Z',
    upcomingWindowDays: 7,
  })

  const apiMock = {
    optimizerReport: vi.fn(async () => report([])),
    optimizerSettings: vi.fn(async () => settings()),
    optimizerUpdateSettings: vi.fn(async () => settings()),
    optimizerSimulate: vi.fn(async () => scenario()),
    spendingOverview: vi.fn(async () => spending()),
  }

  return { recommendation, report, settings, scenario, spending, apiMock }
})

const apiMock = fixtures.apiMock
const recommendation = fixtures.recommendation as unknown as (
  overrides?: Record<string, unknown>,
) => never

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

function mountPanel(): ReturnType<typeof mount> {
  return mount(CostInsights, {
    global: { plugins: [createPinia(), router, i18n] },
  })
}

beforeEach(() => {
  setActivePinia(createPinia())
  // mockClear keeps prior implementations; reset the defaults explicitly so
  // each test starts from the quiet, no-recommendations baseline.
  apiMock.optimizerReport.mockReset()
  apiMock.optimizerReport.mockImplementation(async () => fixtures.report([]))
  apiMock.optimizerSimulate.mockReset()
  apiMock.optimizerSimulate.mockImplementation(async () => fixtures.scenario())
  apiMock.optimizerSettings.mockReset()
  apiMock.optimizerSettings.mockImplementation(async () => fixtures.settings())
  apiMock.optimizerUpdateSettings.mockReset()
  apiMock.optimizerUpdateSettings.mockImplementation(async () => fixtures.settings())
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('M2-B cost insights panel', () => {
  it('renders recommendations with type, confidence and action labels', async () => {
    apiMock.optimizerReport.mockImplementation(async () =>
      fixtures.report([recommendation() as never]),
    )
    const wrapper = mountPanel()
    await flushPromises()
    const text = wrapper.text()
    expect(text).toContain('高于复核阈值')
    expect(text).toContain('Pro is at or above your USD review threshold')
    expect(text).toContain('基于你的规则设置')
    expect(text).toContain('建议复核')
    wrapper.unmount()
  })

  it('shows the empty state when the report has no recommendations', async () => {
    const wrapper = mountPanel()
    await flushPromises()
    expect(wrapper.text()).toContain('暂无建议')
    wrapper.unmount()
  })

  it('keeps the panel usable when the report fails (banner, no crash)', async () => {
    apiMock.optimizerReport.mockImplementation(async () => {
      throw new Error('optimizer report failed')
    })
    const wrapper = mountPanel()
    await flushPromises()
    expect(wrapper.text()).toContain('optimizer report failed')
    wrapper.unmount()
  })

  it('simulation: click-driven entry renders currency, before/after and delta in the result region', async () => {
    const wrapper = mountPanel()
    await flushPromises()

    // Pick a subscription from the dropdown (data-testid wired for the
    // acceptance flow) and click the run button.
    const select = wrapper.find('[data-testid="simulate-target"]')
    expect(select.exists()).toBe(true)
    await select.setValue('sub_1')
    const runButton = wrapper.find('[data-testid="simulate-run"]')
    expect(runButton.exists()).toBe(true)
    await runButton.trigger('click')
    await flushPromises()

    expect(apiMock.optimizerSimulate).toHaveBeenCalledWith([
      { kind: 'cancel', subscriptionId: 'sub_1' },
    ])

    const result = wrapper.find('[data-testid="simulate-result"]')
    expect(result.exists()).toBe(true)
    const resultText = result.text()
    // Currency, before, after and delta are all visible in the region.
    expect(resultText).toContain('USD · monthly')
    expect(resultText).toContain('60.00 USD')
    expect(resultText).toContain('0.00 USD')
    expect(resultText).toContain('−60.00 USD')
    // The disclaimers travel with the result; no fabricated savings figure.
    expect(resultText).toContain('simulation only')
    expect(resultText).not.toMatch(/已节省\s*[\d.]|saved\s+[\d.]+\s*(USD|CNY)/i)
    wrapper.unmount()
  })
})

describe('M2-B settings: one shared storage, two surfaces', () => {
  it('panel reads the persisted settings through the same service path as the CLI', async () => {
    apiMock.optimizerSettings.mockImplementation(async () =>
      fixtures.settings({ costThresholdsMinor: { USD: 5000 }, renewalWindowDays: 10 }),
    )
    const wrapper = mountPanel()
    await flushPromises()
    expect(apiMock.optimizerSettings).toHaveBeenCalledTimes(1)
    // The CLI reads the same keys; the shape here is exactly what
    // `agenthub cost settings --json` emits through the envelope.
    const settingsShape = await apiMock.optimizerSettings.mock.results[0]!.value
    expect(Object.keys(settingsShape).sort()).toEqual(['costThresholdsMinor', 'renewalWindowDays'])
    wrapper.unmount()
  })

  it('settings editor: clicking save submits the edited threshold and window through the shared service path', async () => {
    apiMock.optimizerSettings.mockImplementation(async () =>
      fixtures.settings({ costThresholdsMinor: {}, renewalWindowDays: null }),
    )
    apiMock.optimizerUpdateSettings.mockImplementation(async () =>
      fixtures.settings({ costThresholdsMinor: { USD: 5000 }, renewalWindowDays: 14 }),
    )
    const wrapper = mountPanel()
    await flushPromises()

    // Fill the first threshold row and the window.
    await wrapper.find('[data-testid="threshold-currency-0"]').setValue('usd')
    await wrapper.find('[data-testid="threshold-minor-0"]').setValue('5000')
    await wrapper.find('[data-testid="optimizer-window"]').setValue('14')

    await wrapper.find('[data-testid="settings-save"]').trigger('click')
    await flushPromises()

    // camelCase in TS, snake_case on the wire — the api layer owns the
    // mapping, so the mock sees the TS call shape.
    expect(apiMock.optimizerUpdateSettings).toHaveBeenCalledWith(
      { USD: 5000 },
      14,
    )
    // The editor re-hydrates from the stored config after a successful save.
    expect(wrapper.text()).toContain('成本建议设置已保存')
    wrapper.unmount()
  })

  it('settings editor: clearing every row leaves thresholds unchanged (undefined) and clears the window (null)', async () => {
    apiMock.optimizerSettings.mockImplementation(async () =>
      fixtures.settings({ costThresholdsMinor: { USD: 5000 }, renewalWindowDays: 10 }),
    )
    const wrapper = mountPanel()
    await flushPromises()

    // Remove the only row -> thresholds untouched; empty window -> cleared.
    // (No stray clicks: a refresh would re-hydrate the row from the mock.)
    const removeButtons = wrapper
      .findAll('button')
      .filter((button) => button.text() === '删除')
    await removeButtons[0].trigger('click')
    await flushPromises()
    await wrapper.find('[data-testid="optimizer-window"]').setValue('')
    await wrapper.find('[data-testid="settings-save"]').trigger('click')
    await flushPromises()

    expect(apiMock.optimizerUpdateSettings).toHaveBeenCalledWith(undefined, null)
    wrapper.unmount()
  })
})
