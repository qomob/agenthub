<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { toneForOperationStatus, operationStatusKey } from '../lib/status'
import type { OperationResult } from '../lib/types'
import StatusBadge from './StatusBadge.vue'

withDefaults(
  defineProps<{
    results: OperationResult[]
    errors?: string[]
  }>(),
  { errors: () => [] },
)

const { t } = useI18n()

const RESULT_STATUS_KEY: Record<string, string> = {
  success: 'status.result.success',
  noop: 'status.result.noop',
  skipped: 'status.result.skipped',
  failed: 'status.result.failed',
  partial: 'status.result.partial',
  error: 'status.result.error',
}

function itemStatusTone(status: string): 'success' | 'warn' | 'danger' | 'neutral' {
  if (status === 'success') return 'success'
  if (status === 'noop') return 'neutral'
  if (status === 'partial' || status === 'skipped') return 'warn'
  return 'danger'
}

function itemStatusLabel(status: string): string {
  const key = RESULT_STATUS_KEY[status]
  return key ? t(key) : status
}
</script>

<template>
  <div class="stack-md">
    <div v-for="error in errors" :key="error" class="notice notice-danger">
      <div class="notice-body">
        <p class="notice-title">{{ t('plan.errorDuringApply') }}</p>
        <p class="mono" style="white-space: pre-wrap">{{ error }}</p>
      </div>
    </div>

    <div v-for="result in results" :key="result.operationId" class="surface-inset" style="padding: 11px">
      <div class="row row-wrap" style="justify-content: space-between">
        <div class="row">
          <StatusBadge
            :label="t(operationStatusKey[result.status])"
            :tone="toneForOperationStatus(result.status)"
          />
          <span class="mono muted">{{ result.operationId }}</span>
        </div>
        <span v-if="result.finishedAt" class="hint">{{ result.finishedAt }}</span>
      </div>

      <div v-if="result.items.length > 0" class="table-wrap" style="margin-top: 9px">
        <table class="table table-compact">
          <thead>
            <tr>
              <th scope="col">{{ t('activity.resultStatus') }}</th>
              <th scope="col">{{ t('plan.agent') }}</th>
              <th scope="col">{{ t('plan.target') }}</th>
              <th scope="col">{{ t('activity.message') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="item in result.items" :key="item.planItemId">
              <td>
                <StatusBadge
                  :label="itemStatusLabel(item.status)"
                  :tone="itemStatusTone(item.status)"
                />
              </td>
              <td>{{ item.agentKey }}</td>
              <td class="path-text">{{ item.targetPath || '—' }}</td>
              <td class="text-sm">
                {{ item.message }}
                <p v-if="item.backupPath" class="hint mono">{{ item.backupPath }}</p>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-if="result.diagnostics.length > 0" class="notice notice-info" style="margin-top: 9px">
        <div class="notice-body">
          <p class="notice-title">{{ t('plan.diagnosticsTitle') }}</p>
          <ul class="list-reset stack-sm">
            <li v-for="(diagnostic, index) in result.diagnostics" :key="index">{{ diagnostic }}</li>
          </ul>
        </div>
      </div>
      <p v-else class="hint" style="margin-top: 7px">{{ t('plan.noDiagnostics') }}</p>
    </div>
  </div>
</template>
