<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { strategyKey, toneForPlanAction, planActionKey } from '../lib/status'
import type { OperationPlan } from '../lib/types'
import StatusBadge from './StatusBadge.vue'

defineProps<{ plan: OperationPlan }>()

const { t } = useI18n()
</script>

<template>
  <div class="table-wrap surface-inset">
    <table class="table table-compact">
      <thead>
        <tr>
          <th scope="col">{{ t('plan.action') }}</th>
          <th scope="col">{{ t('plan.capability') }}</th>
          <th scope="col">{{ t('plan.agent') }}</th>
          <th scope="col">{{ t('plan.strategy') }}</th>
          <th scope="col">{{ t('plan.target') }}</th>
        </tr>
      </thead>
      <tbody>
        <template v-for="item in plan.items" :key="item.id">
          <tr>
            <td>
              <StatusBadge
                :label="t(planActionKey[item.action])"
                :tone="toneForPlanAction(item.action)"
              />
            </td>
            <td>
              <div class="row" style="gap: 5px">
                <span style="font-weight: 550">{{ item.capabilityName }}</span>
                <span v-if="item.versionLabel" class="chip">{{ item.versionLabel }}</span>
                <StatusBadge
                  v-if="item.requiresRestart"
                  :label="t('plan.restartBadge')"
                  tone="warn"
                  :dot="false"
                />
              </div>
              <p v-if="item.configKey" class="hint">
                {{ t('mcp.configKey') }}: {{ item.configKey }}
              </p>
            </td>
            <td>
              <div>{{ item.agentName }}</div>
              <p class="hint">{{ item.agentKey }}</p>
            </td>
            <td class="text-sm">{{ t(strategyKey[item.strategy]) }}</td>
            <td class="path-text">{{ item.targetPath || '—' }}</td>
          </tr>
          <tr v-if="item.warnings.length > 0 || item.diff">
            <td :colspan="5" style="padding-top: 0">
              <div class="stack-sm">
                <p v-for="(warning, index) in item.warnings" :key="index" class="hint tone-warn">
                  ⚠ {{ warning }}
                </p>
                <p v-if="item.diff" class="hint">
                  <span class="subtle">{{ t('plan.diff') }}：</span>{{ item.diff }}
                </p>
              </div>
            </td>
          </tr>
        </template>
      </tbody>
    </table>
  </div>
</template>
