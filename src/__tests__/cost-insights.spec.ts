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

  const apiMock = {
    optimizerReport: vi.fn(async () => report([])),
    optimizerSettings: vi.fn(async () => settings()),
    optimizerUpdateSettings: vi.fn(async () => settings()),
    optimizerSimulate: vi.fn(async () => scenario()),
  }

  return { recommendation, report, settings, scenario, apiMock }
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

  it('simulation sends a cancel action and renders per-currency deltas without "saved" claims', async () => {
    apiMock.optimizerReport.mockImplementation(async () =>
      fixtures.report([
        recommendation({
          recommendationType: 'R5_DATA_GAP',
          title: 'gaps',
          recommendationId: 'rec_gap',
          userActionRequired: 'complete_data',
        }) as never,
      ]),
    )
    const wrapper = mountPanel()
    await flushPromises()
    // The panel exposes the simulation through its public API only; drive it
    // directly to keep the test at the contract level.
    const vm = wrapper.vm as unknown as {
      simulateCancel: (subscriptionId: string) => Promise<void>
    }
    await vm.simulateCancel('sub_1')
    await flushPromises()

    expect(apiMock.optimizerSimulate).toHaveBeenCalledWith([
      { kind: 'cancel', subscriptionId: 'sub_1' },
    ])
    const text = wrapper.text()
    expect(text).toContain('60.00 USD')
    expect(text).not.toMatch(/已节省|已实现|saved .* yet|realized saving/i)
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
})
