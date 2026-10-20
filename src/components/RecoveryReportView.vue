<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { formatDateTime } from '../lib/format'
import {
  hasManualRecovery,
  recoveryItemInfo,
  recoveryStatusInfo,
} from '../lib/v31'
import type { RecoveryReport } from '../lib/types'
import Icon from './Icon.vue'
import StatusBadge from './StatusBadge.vue'

const props = defineProps<{ report: RecoveryReport }>()

const { t, locale } = useI18n()

const statusInfo = computed(() => recoveryStatusInfo(props.report.status))
const manual = computed(() => hasManualRecovery(props.report))
const recoveredCount = computed(
  () => props.report.items.filter((item) => item.status === 'success').length,
)

function itemLabel(status: string): string {
  const info = recoveryItemInfo(status)
  return info.key ? t(info.key) : status
}

function itemTone(status: string) {
  return recoveryItemInfo(status).tone
}
</script>

<template>
  <div class="stack-md">
    <div class="row row-wrap" style="gap: 6px">
      <StatusBadge
        :label="statusInfo.key ? t(statusInfo.key) : report.status"
        :tone="statusInfo.tone"
      />
      <span class="chip">{{ t('activity.recoverMode') }}</span>
      <span class="hint mono">{{ report.operationId }}</span>
      <span class="spacer" />
      <span class="hint">{{ formatDateTime(report.finishedAt, locale) }}</span>
    </div>

    <div v-if="manual" class="notice notice-warn" role="alert">
      <Icon class="notice-icon" name="alert-triangle" :size="14" />
      <div class="notice-body">
        <p class="notice-title">{{ t('activity.recoverManualNotice') }}</p>
        <p class="hint">{{ t('activity.recoverManualHint') }}</p>
      </div>
    </div>

    <p v-else class="hint">
      {{ t('activity.recoverDone') }} · {{ recoveredCount }} / {{ report.items.length }}
    </p>

    <div v-if="report.items.length > 0" class="table-wrap surface-inset">
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
          <tr v-for="(item, index) in report.items" :key="`${item.targetPath}-${index}`">
            <td>
              <StatusBadge :label="itemLabel(item.status)" :tone="itemTone(item.status)" />
            </td>
            <td>{{ item.agentKey || '—' }}</td>
            <td class="path-text">{{ item.targetPath || '—' }}</td>
            <td class="text-sm">
              <p v-if="item.status === 'failed'" class="tone-warn">
                {{ t('activity.recoverManualNotice') }}
              </p>
              <p v-if="item.message" class="hint mono" style="white-space: pre-wrap">
                {{ item.message }}
              </p>
              <p v-if="item.backupPath" class="hint mono">{{ item.backupPath }}</p>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-else class="hint">{{ t('activity.recoverResultEmpty') }}</p>

    <div v-if="report.diagnostics.length > 0" class="notice notice-info">
      <div class="notice-body">
        <p class="notice-title">{{ t('activity.recoverDiagnostics') }}</p>
        <ul class="list-reset stack-sm">
          <li v-for="(diagnostic, index) in report.diagnostics" :key="index" class="text-sm mono">
            {{ diagnostic }}
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>
