import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { describe, expect, it, vi } from 'vitest'

import PlanDialog from '../PlanDialog.vue'
import { i18n } from '../../i18n'
import { router } from '../../router'
import { api } from '../../lib/api'
import type { OperationPlan, OperationView } from '../../lib/types'

const OPERATION_ID = 'op-approval-1'
const PLAN_HASH = 'a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90'
const TOKEN = 'memory-only-approval-token-9f8e7d6c'

function plan(): OperationPlan {
  const item = {
    id: 'item-1',
    capabilityId: 'cap-1',
    capabilityName: 'Demo Skill',
    versionId: 'version-1',
    versionLabel: '1.0.0',
    agentId: 'agent-1',
    agentKey: 'codex',
    agentName: 'Codex',
    installKind: 'skill_dir',
    action: 'create' as const,
    strategy: 'copy' as const,
    targetPath: '/tmp/agents/codex/skills/demo',
    sourcePath: '/tmp/library/demo',
    currentHash: null,
    desiredHash: 'hash-desired',
    configKey: null,
    targetFormat: null,
    payload: null,
    warnings: [],
    requiresRestart: false,
    diff: null,
    previousInstallation: null,
  }
  return {
    operationId: OPERATION_ID,
    kind: 'install',
    dryRun: true,
    items: [item],
    warnings: [],
    requiresRestartAgents: [],
    requiresConfirmation: false,
    summary: { create: 1, replace: 0, remove: 0, noop: 0, conflict: 0, blocked: 0 },
    createdAt: '2026-10-20T12:00:00Z',
    planHash: PLAN_HASH,
    expiresAt: '2026-10-20T12:15:00Z',
    approvalState: 'none',
  }
}

function preview(): OperationView {
  return {
    id: OPERATION_ID,
    kind: 'install',
    status: 'planned',
    dryRun: true,
    actor: 'local-user',
    summary: 'install Demo Skill for Codex',
    error: null,
    rollbackOf: null,
    startedAt: '2026-10-20T12:00:00Z',
    finishedAt: null,
    itemCount: 1,
    plan: plan(),
    result: null,
    canRollback: false,
    planHash: PLAN_HASH,
    expiresAt: '2026-10-20T12:15:00Z',
    approvalState: 'none',
    approvedAt: null,
    recoveryState: null,
    steps: [],
  }
}

async function mountDialog() {
  const wrapper = mount(PlanDialog, {
    props: { plans: [plan()] },
    global: { plugins: [createPinia(), router, i18n] },
  })
  await router.isReady()
  await flushPromises()
  return wrapper
}

function clickButton(label: string): void {
  const button = Array.from(document.querySelectorAll('button')).find((element) =>
    element.textContent?.includes(label),
  )
  expect(button, `button "${label}" should exist`).toBeTruthy()
  button!.click()
}

describe('PlanDialog two-step approval', () => {
  it('previews the plan hash, then approves and applies with an in-memory token', async () => {
    const previewSpy = vi.spyOn(api, 'operationsPreview').mockResolvedValue(preview())
    const approveSpy = vi.spyOn(api, 'operationsApprove').mockResolvedValue({
      operationId: OPERATION_ID,
      planHash: PLAN_HASH,
      approvalToken: TOKEN,
      approvedAt: '2026-10-20T12:00:30Z',
      expiresAt: '2026-10-20T12:15:30Z',
    })
    const applySpy = vi.spyOn(api, 'operationsApplyApproved').mockResolvedValue({
      operationId: OPERATION_ID,
      status: 'success',
      items: [],
      diagnostics: [],
      finishedAt: '2026-10-20T12:00:31Z',
    })
    const logSpy = vi.spyOn(console, 'log').mockImplementation(() => {})
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})

    const wrapper = await mountDialog()

    expect(previewSpy).toHaveBeenCalledWith(OPERATION_ID)
    // Plan hash is shown shortened to 12 chars, not invented.
    expect(document.body.textContent).toContain(PLAN_HASH.slice(0, 12))
    expect(document.body.textContent).not.toContain(PLAN_HASH)

    clickButton('批准并应用')
    await flushPromises()

    expect(approveSpy).toHaveBeenCalledWith(OPERATION_ID, PLAN_HASH)
    expect(applySpy).toHaveBeenCalledWith(OPERATION_ID, TOKEN)
    // The one-time token must never leak to logs or persistent storage.
    expect(JSON.stringify(window.localStorage)).not.toContain(TOKEN)
    for (const spy of [logSpy, warnSpy, errorSpy]) {
      for (const call of spy.mock.calls) {
        expect(JSON.stringify(call)).not.toContain(TOKEN)
      }
    }
    expect(document.body.textContent).toContain('以下为引擎返回的真实执行结果。')
    wrapper.unmount()
  })

  it('shows the stale-plan explanation when approval is rejected', async () => {
    vi.spyOn(api, 'operationsPreview').mockResolvedValue(preview())
    vi.spyOn(api, 'operationsApprove').mockRejectedValue({
      code: 'PLAN_STALE',
      message: 'plan hash changed; re-preview the operation and approve again',
    })
    const applySpy = vi.spyOn(api, 'operationsApplyApproved')

    const wrapper = await mountDialog()
    clickButton('批准并应用')
    await flushPromises()

    expect(applySpy).not.toHaveBeenCalled()
    expect(document.body.textContent).toContain('计划已过期或内容已变化，请重新生成预览计划后再批准。')
    expect(document.body.textContent).toContain('PLAN_STALE')
    wrapper.unmount()
  })

  it('blocks approval when the preview cannot be read', async () => {
    vi.spyOn(api, 'operationsPreview').mockRejectedValue(new Error('backend unavailable'))
    const approveSpy = vi.spyOn(api, 'operationsApprove')

    const wrapper = await mountDialog()
    const button = Array.from(document.querySelectorAll('button')).find((element) =>
      element.textContent?.includes('批准并应用'),
    )
    expect(button).toBeTruthy()
    expect(button!.hasAttribute('disabled')).toBe(true)
    expect(document.body.textContent).toContain('无法读取计划预览')
    expect(document.body.textContent).toContain('backend unavailable')

    button!.click()
    await flushPromises()
    expect(approveSpy).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})
