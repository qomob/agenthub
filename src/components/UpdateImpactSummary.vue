<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { formatBytes, formatDateTime } from '../lib/format'
import { auditRiskKey, auditRiskTone, fileChangeKey, fileChangeTone, impactClassKey } from '../lib/health'
import type { UpdateImpact } from '../lib/types'
import Icon from './Icon.vue'
import StatusBadge from './StatusBadge.vue'

const props = withDefaults(
  defineProps<{
    impact: UpdateImpact
    showFiles?: boolean
  }>(),
  { showFiles: false },
)

const { t, locale } = useI18n()

const FILE_LIMIT = 100

const visibleFiles = computed(() => props.impact.fileChanges.slice(0, FILE_LIMIT))
const hiddenFileCount = computed(() =>
  Math.max(0, props.impact.fileChanges.length - visibleFiles.value.length),
)
const riskLabel = computed(() => {
  const key = auditRiskKey[props.impact.risk]
  return key ? t(key) : props.impact.risk
})
const riskTone = computed(() => auditRiskTone[props.impact.risk] ?? 'neutral')

function classLabel(value: string): string {
  const key = impactClassKey[value]
  return key ? t(key) : value
}

function fileLabel(value: string): string {
  const key = fileChangeKey[value]
  return key ? t(key) : value
}
</script>

<template>
  <div class="surface-inset stack-sm" style="padding: 10px">
    <div class="row row-wrap" style="justify-content: space-between">
      <div class="row row-wrap" style="gap: 5px">
        <span class="chip">{{ impact.fromVersionLabel }} → {{ impact.toVersionLabel }}</span>
        <StatusBadge :label="`${t('health.impactRisk')}：${riskLabel}`" :tone="riskTone" :icon="'shield'" />
        <StatusBadge
          v-if="impact.requiresRevalidation"
          :label="t('health.requiresRevalidation')"
          tone="warn"
          icon="alert-triangle"
        />
      </div>
      <span class="hint">{{ formatDateTime(impact.createdAt, locale) }}</span>
    </div>

    <p class="text-sm">{{ impact.summary || '—' }}</p>

    <div v-if="impact.classes.length > 0" class="row row-wrap" style="gap: 3px">
      <span v-for="value in impact.classes" :key="value" class="chip">{{ classLabel(value) }}</span>
    </div>

    <div class="row row-wrap hint" style="gap: 10px">
      <span>
        <Icon name="package" :size="10" />
        {{ t('health.impactAffectedInstallations', { count: impact.affectedInstallationCount }) }}
      </span>
      <span>
        <Icon name="agents" :size="10" />
        {{ t('health.impactAffectedAgents', { count: impact.affectedAgentKeys.length }) }}
      </span>
      <span>
        <Icon name="file" :size="10" />
        {{ t('health.impactFileChanges', { count: impact.fileChanges.length }) }}
      </span>
    </div>

    <p v-if="impact.affectedAgentKeys.length > 0" class="hint">
      {{ t('health.impactTargets') }}：{{ impact.affectedAgentKeys.join(' · ') }}
    </p>
    <p v-if="impact.affectedProjectRoots.length > 0" class="hint path-text">
      {{ impact.affectedProjectRoots.join(' · ') }}
    </p>

    <div v-if="showFiles" class="stack-sm">
      <div v-if="impact.fileChanges.length === 0" class="hint">
        {{ t('health.impactNoFiles') }}
      </div>
      <div v-else class="table-wrap list-scroll" style="max-height: 260px">
        <table class="table table-compact">
          <thead>
            <tr>
              <th scope="col">{{ t('common.path') }}</th>
              <th scope="col">{{ t('health.fileChange') }}</th>
              <th scope="col" style="text-align: right">{{ t('health.sizeBefore') }}</th>
              <th scope="col" style="text-align: right">{{ t('health.sizeAfter') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="file in visibleFiles" :key="file.path">
              <td class="path-text">{{ file.path }}</td>
              <td>
                <StatusBadge
                  :label="fileLabel(file.change)"
                  :tone="fileChangeTone[file.change] ?? 'neutral'"
                  :dot="false"
                />
              </td>
              <td class="num">{{ formatBytes(file.sizeBefore) }}</td>
              <td class="num">{{ formatBytes(file.sizeAfter) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <p v-if="hiddenFileCount > 0" class="hint">
        {{ t('health.impactFilesTruncated', { shown: visibleFiles.length, total: impact.fileChanges.length }) }}
      </p>
    </div>
  </div>
</template>
