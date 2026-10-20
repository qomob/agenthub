<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { planHasBlockedItems, planNeedsForce } from '../lib/status'
import type { OperationPlan } from '../lib/types'
import Icon from './Icon.vue'
import PlanItemsTable from './PlanItemsTable.vue'

const props = defineProps<{ plan: OperationPlan }>()
const { t } = useI18n()

const summary = computed(() => props.plan.summary)
const chips = computed(() => [
  { key: 'create', label: t('plan.summaryCreate'), tone: 'success', count: summary.value.create },
  { key: 'replace', label: t('plan.summaryReplace'), tone: 'info', count: summary.value.replace },
  { key: 'remove', label: t('plan.summaryRemove'), tone: 'warn', count: summary.value.remove },
  { key: 'noop', label: t('plan.summaryNoop'), tone: 'neutral', count: summary.value.noop },
  { key: 'conflict', label: t('plan.summaryConflict'), tone: 'warn', count: summary.value.conflict },
  { key: 'blocked', label: t('plan.summaryBlocked'), tone: 'danger', count: summary.value.blocked },
])
</script>

<template>
  <div class="stack-md">
    <div class="row row-wrap" style="gap: 6px">
      <span
        v-for="chip in chips.filter((item) => item.count > 0)"
        :key="chip.key"
        class="badge"
        :class="`badge-${chip.tone}`"
      >
        {{ chip.label }} {{ chip.count }}
      </span>
      <span v-if="plan.dryRun" class="badge badge-outline">{{ t('plan.dryRun') }}</span>
      <span class="spacer" />
      <span class="hint mono">{{ plan.operationId }}</span>
    </div>

    <div v-if="planHasBlockedItems(plan)" class="notice notice-warn">
      <Icon class="notice-icon" name="alert-triangle" :size="14" />
      <div class="notice-body">
        <p class="notice-title">{{ t('plan.blockedTitle') }}</p>
        <p>{{ t('plan.blockedNotice') }}</p>
        <p class="notice-title" style="margin-top: 7px">{{ t('plan.skippedNotice') }}</p>
        <ul class="list-reset stack-sm">
          <li v-for="item in plan.items.filter((entry) => entry.action === 'blocked')" :key="item.id">
            {{ item.capabilityName }} · {{ item.agentName }}
            <span v-if="item.warnings.length > 0" class="hint">— {{ item.warnings.join('; ') }}</span>
          </li>
        </ul>
        <p class="hint" style="margin-top: 4px">{{ t('plan.skippedHint') }}</p>
      </div>
    </div>

    <div v-if="plan.warnings.length > 0" class="notice notice-warn">
      <Icon class="notice-icon" name="alert-triangle" :size="14" />
      <div class="notice-body">
        <p class="notice-title">{{ t('plan.warningsTitle') }}</p>
        <ul class="list-reset stack-sm">
          <li v-for="(warning, index) in plan.warnings" :key="index">{{ warning }}</li>
        </ul>
      </div>
    </div>

    <div v-if="plan.requiresRestartAgents.length > 0" class="notice notice-info">
      <Icon class="notice-icon" name="refresh" :size="14" />
      <div class="notice-body">
        <p class="notice-title">{{ t('plan.restartTitle') }}</p>
        <p>{{ t('plan.restartHint') }}</p>
        <ul class="list-reset stack-sm" style="margin-top: 5px">
          <li v-for="(agent, index) in plan.requiresRestartAgents" :key="index">{{ agent }}</li>
        </ul>
      </div>
    </div>

    <PlanItemsTable :plan="plan" />
    <p v-if="planNeedsForce(plan)" class="hint">{{ t('plan.forceHint') }}</p>
  </div>
</template>
