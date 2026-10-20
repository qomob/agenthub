import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'

import RecoveryReportView from '../RecoveryReportView.vue'
import { i18n } from '../../i18n'
import type { RecoveryReport } from '../../lib/types'

function report(overrides: Partial<RecoveryReport> = {}): RecoveryReport {
  return {
    operationId: 'op-1',
    status: 'recovered',
    items: [
      {
        planItemId: '',
        agentKey: 'codex',
        targetPath: '/tmp/agents/codex/skills/demo',
        status: 'success',
        message: 'Restored pre-operation content',
        backupPath: '/tmp/snapshots/op-1/demo',
        installationId: null,
      },
    ],
    diagnostics: [],
    finishedAt: '2026-10-20T12:00:00Z',
    ...overrides,
  }
}

function mountReport(value: RecoveryReport) {
  return mount(RecoveryReportView, {
    props: { report: value },
    global: { plugins: [i18n] },
  })
}

describe('RecoveryReportView', () => {
  it('renders per-target recovery outcomes', () => {
    const wrapper = mountReport(report())
    const text = wrapper.text()
    expect(text).toContain('已恢复')
    expect(text).toContain('/tmp/agents/codex/skills/demo')
    expect(text).toContain('codex')
    expect(text).toContain('成功')
    expect(text).toContain('恢复流程已结束')
    expect(text).not.toContain('文件已被其他程序修改，已暂停覆盖。')
    wrapper.unmount()
  })

  it('pauses overwrite and shows explicit copy for needs_manual_recovery', () => {
    const wrapper = mountReport(
      report({
        status: 'needs_manual_recovery',
        items: [
          {
            planItemId: '',
            agentKey: 'claude',
            targetPath: '/tmp/agents/claude/skills/demo',
            status: 'failed',
            message: 'External modification detected; manual recovery required',
            backupPath: null,
            installationId: null,
          },
          {
            planItemId: '',
            agentKey: 'codex',
            targetPath: '/tmp/agents/codex/skills/demo',
            status: 'success',
            message: 'Restored pre-operation content',
            backupPath: null,
            installationId: null,
          },
        ],
        diagnostics: ['/tmp/agents/claude/skills/demo was modified outside AgentHub'],
      }),
    )
    const text = wrapper.text()
    expect(text).toContain('文件已被其他程序修改，已暂停覆盖。')
    expect(text).toContain('需人工恢复')
    expect(text).toContain('外部修改会保留')
    expect(text).toContain('/tmp/agents/claude/skills/demo')
    expect(text).toContain('恢复诊断')
    wrapper.unmount()
  })
})
