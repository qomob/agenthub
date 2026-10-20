import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { describe, expect, it, vi } from 'vitest'

import HealthView from '../pages/HealthView.vue'
import SkillHealthPanel from '../components/SkillHealthPanel.vue'
import { i18n } from '../i18n'
import { router } from '../router'

const health = {
  capabilityId: 'cap-1',
  slug: 'demo',
  name: 'Demo Skill',
  description: 'demo',
  versionLabel: '1.2.0',
  status: 'needs_attention' as const,
  reasons: ['static audit reported high risk', 'missing runtime dependency'],
  audit: {
    id: 'audit-1',
    capabilityId: 'cap-1',
    versionId: 'v1',
    versionLabel: '1.2.0',
    kind: 'static',
    status: 'warn' as const,
    risk: 'high' as const,
    summary: 'one high finding',
    findings: [
      {
        id: 'f1',
        severity: 'high' as const,
        category: 'security' as const,
        title: 'pipe to shell',
        detail: 'script pipes curl to sh',
        path: 'scripts/install.sh',
        suggestion: 'download then verify',
      },
    ],
    createdAt: '2026-10-20T00:00:00Z',
  },
  environment: null,
  validations: [],
  installationCount: 2,
  agentKeys: ['codex', 'claude'],
  verifiedAgentKeys: ['codex'],
  lastCheckedAt: '2026-10-20T00:00:00Z',
}

const skill = {
  id: 'cap-1',
  slug: 'demo',
  name: 'Demo Skill',
  description: 'demo',
  version: '1.2.0',
  contentHash: 'hash',
  tags: [],
  license: null,
  source: null,
  currentVersionId: 'v1',
  sizeBytes: 10,
  fileCount: 1,
  createdAt: '2026-10-20T00:00:00Z',
  updatedAt: '2026-10-20T00:00:00Z',
  installations: [],
  updateAvailable: false,
  driftCount: 0,
  conflict: null,
}

vi.mock('../lib/api', () => ({
  api: {
    healthOverview: vi.fn(async () => ({
      summary: {
        total: 1,
        verified: 0,
        unverified: 0,
        needsAttention: 1,
        failed: 0,
        notDeployed: 0,
        lastCheckedAt: '2026-10-20T00:00:00Z',
      },
      capabilities: [
        {
          capabilityId: 'cap-1',
          slug: 'demo',
          name: 'Demo Skill',
          description: 'demo',
          versionLabel: '1.2.0',
          status: 'needs_attention',
          reasons: ['static audit reported high risk'],
          audit: null,
          environment: null,
          validations: [],
          installationCount: 2,
          agentKeys: ['codex'],
          verifiedAgentKeys: [],
          lastCheckedAt: '2026-10-20T00:00:00Z',
        },
      ],
    })),
    agentHealth: vi.fn(async () => ({ checkedAt: '', agents: [] })),
    skillsHealth: vi.fn(async () => health),
    validationsList: vi.fn(async () => []),
    skillsAudit: vi.fn(),
    skillsDependencies: vi.fn(),
    skillsPreviewUpdate: vi.fn(),
    validationsRecord: vi.fn(),
  },
  normalizeError: (error: unknown, fallback: string) =>
    typeof error === 'string' ? error : fallback,
  isDesktop: () => true,
  NOT_TAURI_MESSAGE: '',
  ApiError: class ApiError extends Error {},
}))

describe('health UI smoke', () => {
  it('mounts the Health page with the overview', async () => {
    await router.push('/health')
    await router.isReady()
    const wrapper = mount(HealthView, {
      global: { plugins: [createPinia(), router, i18n] },
    })
    await flushPromises()
    expect(wrapper.text()).toContain('健康摘要')
    expect(wrapper.text()).toContain('Demo Skill')
    expect(wrapper.text()).toContain('待处理')
    wrapper.unmount()
  })

  it('mounts the SkillHealthPanel with audit findings and empty validation state', async () => {
    const wrapper = mount(SkillHealthPanel, {
      props: {
        skill,
        versions: [
          {
            id: 'v1',
            capabilityId: 'cap-1',
            versionLabel: '1.2.0',
            contentHash: 'hash',
            storagePath: '/tmp',
            sizeBytes: 10,
            fileCount: 1,
            notes: null,
            createdAt: '2026-10-20T00:00:00Z',
            isCurrent: true,
          },
        ],
      },
      global: { plugins: [createPinia(), i18n] },
    })
    await flushPromises()
    const text = wrapper.text()
    expect(text).toContain('安全审计')
    expect(text).toContain('pipe to shell')
    expect(text).toContain('验证证据')
    expect(text).toContain('当前版本已是目标版本')
    wrapper.unmount()
  })
})
