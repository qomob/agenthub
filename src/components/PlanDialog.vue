<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { api, toApiError } from '../lib/api'
import { formatDateTime, shortHash } from '../lib/format'
import { operationKindKey, planHasBlockedItems, planNeedsForce } from '../lib/status'
import type { OperationPlan, OperationResult, OperationView, UpdateImpact } from '../lib/types'
import {
  approvalErrorHintKey,
  approvalStateInfo,
  formatPlanHash,
  isDeadlineExpired,
  resolveApprovalState,
} from '../lib/v31'
import BaseModal from './BaseModal.vue'
import ErrorBanner from './ErrorBanner.vue'
import Icon from './Icon.vue'
import LoadingState from './LoadingState.vue'
import OperationResultView from './OperationResultView.vue'
import PlanPreview from './PlanPreview.vue'
import StatusBadge from './StatusBadge.vue'
import UpdateImpactSummary from './UpdateImpactSummary.vue'

const props = withDefaults(
  defineProps<{ plans: OperationPlan[]; impacts?: UpdateImpact[] }>(),
  { impacts: () => [] },
)
const emit = defineEmits<{ close: []; applied: [] }>()

const { t, locale } = useI18n()
const router = useRouter()

const phase = ref<'idle' | 'approving' | 'writing'>('idle')
const applied = ref(false)
const results = ref<OperationResult[]>([])
const errors = ref<string[]>([])

const previews = ref<Record<string, OperationView>>({})
const previewErrors = ref<Record<string, string>>({})
const previewLoading = ref(false)
const previewLoaded = ref(false)

const busy = computed(() => phase.value !== 'idle')
const multi = computed(() => props.plans.length > 1)
const hasBlocked = computed(() =>
  props.plans.some((plan) => planHasBlockedItems(planFor(plan))),
)
const conflictPlans = computed(() => props.plans.filter((plan) => planNeedsForce(planFor(plan))))
const hasConflict = computed(() => conflictPlans.value.length > 0)

/** Prefer the freshly previewed plan; fall back to the dialog input. */
function planFor(plan: OperationPlan): OperationPlan {
  return previews.value[plan.operationId]?.plan ?? plan
}

function previewFor(plan: OperationPlan): OperationView | null {
  return previews.value[plan.operationId] ?? null
}

function previewErrorFor(plan: OperationPlan): string | null {
  return previewErrors.value[plan.operationId] ?? null
}

function planHashFor(plan: OperationPlan): string | null {
  return previewFor(plan)?.planHash ?? plan.planHash ?? null
}

function expiresAtFor(plan: OperationPlan): string | null {
  return previewFor(plan)?.expiresAt ?? plan.expiresAt ?? null
}

function approvalStateFor(plan: OperationPlan) {
  const preview = previewFor(plan)
  if (preview) return resolveApprovalState(preview)
  if (plan.approvalState === 'approved' && isDeadlineExpired(plan.expiresAt)) return 'expired'
  return plan.approvalState === 'expired' ? 'expired' : plan.approvalState === 'approved' ? 'approved' : 'none'
}

function stepsFor(plan: OperationPlan) {
  return previewFor(plan)?.steps ?? []
}

function approvalLabel(plan: OperationPlan): string {
  const state = approvalStateFor(plan)
  const info = approvalStateInfo(state)
  return info.key ? t(info.key) : state
}

const previewsReady = computed(
  () =>
    previewLoaded.value &&
    props.plans.every((plan) => {
      if (previewErrorFor(plan)) return false
      return Boolean(planHashFor(plan))
    }),
)

const canApply = computed(() => !busy.value && previewsReady.value && !hasConflict.value)

const title = computed(() => {
  if (props.plans.length === 1) {
    return `${t('plan.title')} · ${t(operationKindKey[planFor(props.plans[0]).kind])}`
  }
  return `${t('plan.title')} · ${props.plans.length} ${t('plan.plansCount')}`
})

const footerHint = computed(() => {
  if (phase.value === 'approving') return t('plan.approving')
  if (phase.value === 'writing') return t('plan.writing')
  return ''
})

const applyLabel = computed(() => {
  if (phase.value === 'approving') return t('plan.approving')
  if (phase.value === 'writing') return t('plan.writing')
  return t('plan.approveApply')
})

onMounted(() => {
  void loadPreviews()
})

/**
 * Step 1 of the two-step approval: read the immutable plan, its hash, expiry
 * and durable journal from the backend. Nothing is written here.
 */
async function loadPreviews(): Promise<void> {
  previewLoading.value = true
  previewLoaded.value = false
  previewErrors.value = {}
  try {
    await Promise.all(
      props.plans.map(async (plan) => {
        try {
          const preview = await api.operationsPreview(plan.operationId)
          if (!preview.planHash) {
            previewErrors.value = {
              ...previewErrors.value,
              [plan.operationId]: t('plan.previewEmpty'),
            }
            return
          }
          previews.value = { ...previews.value, [plan.operationId]: preview }
        } catch (error) {
          previewErrors.value = {
            ...previewErrors.value,
            [plan.operationId]: toApiError(error, t('errors.previewFailed')).message,
          }
        }
      }),
    )
  } finally {
    previewLoading.value = false
    previewLoaded.value = true
  }
}

/**
 * Step 2: approve (binding the full plan hash), then apply with the one-time
 * token. The token lives only in this loop's local variable — it is never
 * assigned to reactive state, logged, or written to localStorage.
 */
async function approveAndApply(): Promise<void> {
  if (!canApply.value) return
  errors.value = []
  results.value = []
  try {
    for (const plan of props.plans) {
      const planHash = planHashFor(plan)
      if (!planHash) {
        errors.value = [...errors.value, t('plan.previewEmpty')]
        continue
      }
      try {
        phase.value = 'approving'
        const grant = await api.operationsApprove(plan.operationId, planHash)
        phase.value = 'writing'
        if (!grant.approvalToken) {
          errors.value = [...errors.value, t('plan.approvalFailedHint')]
          continue
        }
        const result = await api.operationsApplyApproved(plan.operationId, grant.approvalToken)
        results.value = [...results.value, result]
      } catch (error) {
        const apiError = toApiError(error, t('errors.approvalFailed'))
        const hint = t(approvalErrorHintKey(apiError.code))
        const code = apiError.code ? `（${apiError.code}）` : ''
        errors.value = [...errors.value, `${hint}${code}`]
      }
    }
  } finally {
    phase.value = 'idle'
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
  if (busy.value) return
  emit('close')
}

function reload(): void {
  void loadPreviews()
}
</script>

<template>
  <BaseModal :title="title" :subtitle="t('plan.subtitle')" size="xl" @close="onClose">
    <div v-if="!applied" class="stack-md">
      <div v-if="multi" class="notice notice-info">
        <Icon class="notice-icon" name="info" :size="14" />
        <p>{{ t('plan.multiPlanHint') }}</p>
      </div>

      <div v-if="impacts.length > 0" class="surface-inset stack-sm" style="padding: 11px">
        <div>
          <p class="card-title">{{ t('plan.impactTitle') }}</p>
          <p class="card-subtitle">{{ t('plan.impactLead') }}</p>
        </div>
        <UpdateImpactSummary v-for="impact in impacts" :key="impact.id" :impact="impact" />
        <p v-if="impacts.some((impact) => impact.requiresRevalidation)" class="hint tone-warn">
          {{ t('plan.impactRequiresRevalidation') }}
        </p>
      </div>

      <p v-if="hasBlocked" class="notice notice-warn">
        {{ t('plan.blockedNotice') }}
      </p>

      <div v-if="hasConflict" class="notice notice-warn" role="alert">
        <Icon class="notice-icon" name="alert-triangle" :size="14" />
        <div class="notice-body">
          <p class="notice-title">{{ t('plan.conflictApprovalTitle') }}</p>
          <p>{{ t('plan.conflictApprovalHint') }}</p>
        </div>
      </div>

      <LoadingState v-if="previewLoading" :label="t('plan.previewLoading')" />

      <div v-else class="stack-md">
        <ErrorBanner
          v-if="Object.keys(previewErrors).length > 0"
          :message="Object.values(previewErrors).join('\n')"
          :title="t('errors.previewFailed')"
          :retry-label="t('common.retry')"
          @retry="reload"
        />

        <section v-for="plan in plans" :key="plan.operationId" class="stack-sm">
          <div v-if="multi" class="row row-wrap" style="justify-content: space-between">
            <span class="card-title">{{ t(operationKindKey[planFor(plan).kind]) }}</span>
            <span class="hint mono">{{ plan.operationId }}</span>
          </div>
          <PlanPreview :plan="planFor(plan)" />

          <div class="surface-inset stack-sm" style="padding: 11px">
            <div class="row row-wrap" style="gap: 8px">
              <div class="field" style="min-width: 200px">
                <span class="label">{{ t('plan.planHash') }}</span>
                <span
                  class="mono"
                  :title="planHashFor(plan) ?? t('common.na')"
                >{{ formatPlanHash(planHashFor(plan)) }}</span>
              </div>
              <div class="field" style="min-width: 180px">
                <span class="label">{{ t('plan.expiresAt') }}</span>
                <span :class="{ 'tone-danger': isDeadlineExpired(expiresAtFor(plan)) }">
                  {{ expiresAtFor(plan) ? formatDateTime(expiresAtFor(plan), locale) : t('common.na') }}
                </span>
              </div>
              <div class="field">
                <span class="label">{{ t('plan.approvalState') }}</span>
                <StatusBadge
                  :label="approvalLabel(plan)"
                  :tone="approvalStateInfo(approvalStateFor(plan)).tone"
                />
              </div>
              <div v-if="previewFor(plan)?.approvedAt" class="field">
                <span class="label">{{ t('plan.approvedAt') }}</span>
                <span>{{ formatDateTime(previewFor(plan)?.approvedAt, locale) }}</span>
              </div>
            </div>
            <p class="hint">{{ t('plan.planHashHint') }}</p>
            <p v-if="approvalStateFor(plan) === 'expired'" class="hint tone-warn">
              {{ t('plan.approvalExpiredHint') }}
            </p>
            <p class="hint tone-warn">{{ t('plan.tokenMemoryHint') }}</p>
          </div>

          <div class="surface-inset stack-sm" style="padding: 11px">
            <p class="card-title">{{ t('plan.stepsTitle') }}</p>
            <div v-if="stepsFor(plan).length > 0" class="table-wrap">
              <table class="table table-compact">
                <thead>
                  <tr>
                    <th scope="col">{{ t('plan.stepSequence') }}</th>
                    <th scope="col">{{ t('plan.target') }}</th>
                    <th scope="col">{{ t('plan.stepState') }}</th>
                    <th scope="col">{{ t('plan.stepBeforeHash') }}</th>
                    <th scope="col">{{ t('plan.stepAfterHash') }}</th>
                    <th scope="col">{{ t('plan.stepStartedAt') }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="step in stepsFor(plan)" :key="step.id">
                    <td class="num">{{ step.sequenceNo }}</td>
                    <td class="path-text">{{ step.targetPath }}</td>
                    <td>
                      <span class="chip">{{ step.state }}</span>
                      <p v-if="step.errorCode" class="hint tone-danger mono">{{ step.errorCode }}</p>
                    </td>
                    <td class="mono text-xs">{{ shortHash(step.beforeHash, 12) }}</td>
                    <td class="mono text-xs">{{ shortHash(step.afterHash, 12) }}</td>
                    <td class="text-xs">{{ formatDateTime(step.startedAt, locale) }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <p v-else class="hint">{{ t('plan.stepsEmpty') }}</p>
          </div>
        </section>
      </div>

      <p v-if="results.length === 0 && errors.length === 0 && previewsReady" class="hint">
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
        <button type="button" class="btn" :disabled="busy || previewLoading" @click="reload">
          <span v-if="previewLoading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('common.refresh') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="!canApply" @click="approveAndApply">
          <span v-if="busy" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="check" :size="13" />
          {{ applyLabel }}
        </button>
        <p v-if="footerHint" class="hint" style="width: 100%; text-align: right">
          {{ footerHint }}
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
