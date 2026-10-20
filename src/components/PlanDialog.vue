<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { api, normalizeError } from '../lib/api'
import { operationKindKey, planHasBlockedItems, planNeedsForce } from '../lib/status'
import type { OperationPlan, OperationResult } from '../lib/types'
import { useUiStore } from '../stores/ui'
import BaseModal from './BaseModal.vue'
import Icon from './Icon.vue'
import OperationResultView from './OperationResultView.vue'
import PlanPreview from './PlanPreview.vue'

const props = defineProps<{ plans: OperationPlan[] }>()
const emit = defineEmits<{ close: []; applied: [] }>()

const { t } = useI18n()
const router = useRouter()
const ui = useUiStore()

const busy = ref(false)
const force = ref(false)
const applied = ref(false)
const results = ref<OperationResult[]>([])
const errors = ref<string[]>([])

const multi = computed(() => props.plans.length > 1)
const hasConflict = computed(() => props.plans.some((plan) => planNeedsForce(plan)))
const hasBlocked = computed(() => props.plans.some((plan) => planHasBlockedItems(plan)))
// Blocked items are skipped by the engine (reported as "skipped"), so they do
// not prevent applying; only unresolved conflicts require the force checkbox.
const canApply = computed(() => !busy.value && (!hasConflict.value || force.value))

const title = computed(() => {
  if (props.plans.length === 1) {
    return `${t('plan.title')} · ${t(operationKindKey[props.plans[0].kind])}`
  }
  return `${t('plan.title')} · ${props.plans.length} ${t('plan.plansCount')}`
})

async function apply(): Promise<void> {
  if (!canApply.value) return
  busy.value = true
  errors.value = []
  results.value = []
  try {
    for (const plan of props.plans) {
      try {
        const result = await api.applyOperation(plan.operationId, force.value)
        results.value = [...results.value, result]
      } catch (error) {
        errors.value = [...errors.value, normalizeError(error, t('errors.applyFailed'))]
      }
    }
  } finally {
    busy.value = false
    if (results.value.length > 0 || errors.value.length > 0) {
      applied.value = true
      emit('applied')
    }
  }
}

function goActivity(): void {
  emit('close')
  router.push({ name: 'activity' })
}

function onAppliedDone(): void {
  emit('close')
}

function onClose(): void {
  emit('close')
}
</script>

<template>
  <BaseModal :title="title" :subtitle="t('plan.subtitle')" size="xl" @close="onClose">
    <div v-if="!applied" class="stack-md">
      <div v-if="multi" class="notice notice-info">
        <Icon class="notice-icon" name="info" :size="14" />
        <p>{{ t('plan.multiPlanHint') }}</p>
      </div>

      <p v-if="hasBlocked" class="notice notice-warn">
        {{ t('plan.blockedNotice') }}
      </p>

      <section v-for="plan in plans" :key="plan.operationId" class="stack-sm">
        <div v-if="multi" class="row row-wrap" style="justify-content: space-between">
          <span class="card-title">{{ t(operationKindKey[plan.kind]) }}</span>
          <span class="hint mono">{{ plan.operationId }}</span>
        </div>
        <PlanPreview :plan="plan" />
      </section>

      <div v-if="hasConflict" class="surface-inset" style="padding: 11px">
        <label class="checkbox-row">
          <input v-model="force" type="checkbox" class="checkbox" />
          <span>
            <span style="font-weight: 600">{{ t('plan.forceLabel') }}</span>
            <span class="hint" style="display: block">{{ t('plan.forceHint') }}</span>
          </span>
        </label>
        <p v-if="!force" class="hint tone-warn" style="margin-top: 6px">
          {{ t('plan.requireForce') }}
        </p>
      </div>

      <p v-if="results.length === 0 && errors.length === 0" class="hint">
        {{ t('plan.subtitle') }}
      </p>
    </div>

    <div v-else class="stack-md">
      <div class="notice notice-info">
        <Icon class="notice-icon" name="info" :size="14" />
        <p>{{ t('plan.resultSubtitle') }}</p>
      </div>
      <OperationResultView :results="results" :errors="errors" />
    </div>

    <template #footer>
      <template v-if="!applied">
        <button type="button" class="btn" :disabled="busy" @click="onClose">
          {{ t('common.cancel') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="!canApply" @click="apply">
          <span v-if="busy" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="check" :size="13" />
          {{ busy ? t('plan.applying') : t('plan.apply') }}
        </button>
        <p v-if="busy" class="hint" style="width: 100%; text-align: right">
          {{ t('plan.subtitle') }}
        </p>
      </template>
      <template v-else>
        <button type="button" class="btn" @click="onAppliedDone">
          {{ t('common.close') }}
        </button>
        <button type="button" class="btn btn-primary" @click="goActivity">
          <Icon name="activity" :size="13" />
          {{ t('plan.viewActivity') }}
        </button>
      </template>
    </template>
  </BaseModal>
</template>
