<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import BaseModal from '../components/BaseModal.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import EmptyState from '../components/EmptyState.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { api, ApiError, normalizeError } from '../lib/api'
import {
  formatMoneyMinor,
  stackDetectionStateKey,
  stackSourceKey,
  stackSubscriptionStateKey,
} from '../lib/stack'
import type { CurrencyBreakdown, SpendingOverview, StackItemView } from '../lib/types'
import { useRefreshOnActivate } from '../composables/useRefreshOnActivate'
import { useStackStore } from '../stores/stack'
import { useUiStore } from '../stores/ui'

const { t } = useI18n()
const router = useRouter()
const stackStore = useStackStore()
const ui = useUiStore()

const items = computed(() => stackStore.items)
const loading = computed(() => stackStore.loading)
const error = computed(() => stackStore.error)
const spending = computed<SpendingOverview | null>(() => stackStore.spending)
const spendingError = computed(() => stackStore.spendingError)

const refreshing = ref(false)
const refreshNote = ref<string | null>(null)

// -- item editor -----------------------------------------------------------

const editorOpen = ref(false)
const editingId = ref<string | null>(null)
const saving = ref(false)
const editorError = ref<string | null>(null)
const draft = reactive({
  name: '',
  vendor: '',
  overlapGroup: '',
  notes: '',
})

// -- merge / unmerge / delete ----------------------------------------------

const mergeTarget = ref<StackItemView | null>(null)
const mergeIntoId = ref('')
const merging = ref(false)
const mergeError = ref<string | null>(null)

const unmergeTarget = ref<StackItemView | null>(null)
const unmerging = ref(false)

const deleteTarget = ref<StackItemView | null>(null)
const deleting = ref(false)
const deleteError = ref<string | null>(null)

const mergeCandidates = computed(() =>
  items.value.filter(
    (item) =>
      mergeTarget.value !== null &&
      item.id !== mergeTarget.value.id &&
      !item.mergedInto,
  ),
)

// -- spending helpers --------------------------------------------------------

function currencyRow(breakdown: CurrencyBreakdown): { monthly: string; annual: string } {
  return {
    monthly: formatMoneyMinor(breakdown.monthlyEquivalentMinor, breakdown.currency) ?? '—',
    annual: formatMoneyMinor(breakdown.annualEquivalentMinor, breakdown.currency) ?? '—',
  }
}

const hasSpendingData = computed(() => (spending.value?.counts.total ?? 0) > 0)

/**
 * Refresh on every real activation (mount, keep-alive re-entry, window focus).
 * The store TTL decides whether that read hits the cache, so re-entering the
 * page within 15s costs nothing — but returning after that always re-checks,
 * which is what makes an external CLI write visible without a restart.
 */
useRefreshOnActivate(() => {
  void refreshFromSource()
})

async function refreshFromSource(): Promise<void> {
  try {
    await stackStore.load()
  } catch {
    // error state is surfaced by the banner
  }
  await stackStore.loadSpending()
}

async function refreshDetections(): Promise<void> {
  refreshing.value = true
  refreshNote.value = null
  try {
    const report = await api.stackRefreshDetections()
    stackStore.invalidateSpending()
    await stackStore.load(true)
    await stackStore.loadSpending()
    ui.notify('success', t('stack.refreshDone'), {
      detail: t('stack.refreshDetail', {
        total: report.detectionsTotal,
        created: report.itemsCreated,
        attached: report.itemsAttached,
      }),
    })
  } catch (err) {
    ui.notifyError(t('stack.errors.refreshFailed'), String(err))
  } finally {
    refreshing.value = false
  }
}

function openEditor(item?: StackItemView): void {
  editingId.value = item?.id ?? null
  draft.name = item?.name ?? ''
  draft.vendor = item?.vendor ?? ''
  draft.overlapGroup = item?.overlapGroup ?? ''
  draft.notes = item?.notes ?? ''
  editorError.value = null
  editorOpen.value = true
}

async function save(): Promise<void> {
  if (!draft.name.trim()) {
    editorError.value = t('stack.fieldNameRequired')
    return
  }
  saving.value = true
  editorError.value = null
  try {
    await api.stackUpsertItem({
      id: editingId.value ?? undefined,
      name: draft.name.trim(),
      vendor: draft.vendor.trim() || undefined,
      overlapGroup: draft.overlapGroup.trim() || undefined,
      notes: draft.notes.trim() || undefined,
    })
    editorOpen.value = false
    ui.notify('success', t('stack.saved'), { detail: draft.name.trim() })
    stackStore.invalidateSpending()
    await stackStore.load(true)
    await stackStore.loadSpending()
  } catch (err) {
    editorError.value = normalize(err)
  } finally {
    saving.value = false
  }
}

async function confirmItem(item: StackItemView): Promise<void> {
  try {
    await api.stackConfirmItem(item.id)
    ui.notify('success', t('stack.badgeConfirmed'), { detail: item.name })
    await stackStore.load(true)
  } catch (err) {
    ui.notifyError(t('stack.errors.confirmFailed'), normalize(err))
  }
}

function openMerge(item: StackItemView): void {
  mergeTarget.value = item
  mergeIntoId.value = ''
  mergeError.value = null
}

async function doMerge(): Promise<void> {
  const child = mergeTarget.value
  if (!child || !mergeIntoId.value) return
  merging.value = true
  mergeError.value = null
  try {
    await api.stackMergeItems(child.id, mergeIntoId.value)
    mergeTarget.value = null
    ui.notify('success', t('stack.mergeDone'))
    stackStore.invalidateSpending()
    await stackStore.load(true)
    await stackStore.loadSpending()
  } catch (err) {
    mergeError.value = normalize(err)
  } finally {
    merging.value = false
  }
}

async function doUnmerge(): Promise<void> {
  const item = unmergeTarget.value
  if (!item) return
  unmerging.value = true
  try {
    await api.stackUnmergeItem(item.id)
    unmergeTarget.value = null
    ui.notify('success', t('stack.unmergeDone'))
    stackStore.invalidateSpending()
    await stackStore.load(true)
    await stackStore.loadSpending()
  } catch (err) {
    ui.notifyError(t('stack.errors.unmergeFailed'), normalize(err))
    unmergeTarget.value = null
  } finally {
    unmerging.value = false
  }
}

async function doDelete(): Promise<void> {
  const item = deleteTarget.value
  if (!item) return
  deleting.value = true
  deleteError.value = null
  try {
    await api.stackDeleteItem(item.id)
    deleteTarget.value = null
    ui.notify('success', t('stack.deleted'), { detail: item.name })
    stackStore.invalidateSpending()
    await stackStore.load(true)
    await stackStore.loadSpending()
  } catch (err) {
    deleteError.value = normalize(err)
  } finally {
    deleting.value = false
  }
}

function normalize(err: unknown): string {
  const code = err instanceof ApiError ? err.code : null
  if (code === 'RESOURCE_CONFLICT') {
    return t('stack.errors.resourceConflict')
  }
  return normalizeError(err, err instanceof Error ? err.message : String(err))
}

function openDetail(item: StackItemView): void {
  void router.push({ name: 'stack-detail', params: { id: item.id } })
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('stack.title')" :subtitle="t('stack.subtitle')">
      <template #actions>
        <button type="button" class="btn" :disabled="refreshing" @click="refreshDetections">
          <span v-if="refreshing" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ refreshing ? t('stack.refreshing') : t('stack.refresh') }}
        </button>
        <button type="button" class="btn btn-primary" @click="openEditor()">
          <Icon name="plus" :size="13" />
          {{ t('stack.addItem') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner
        v-if="error"
        :message="error"
        :title="t('stack.loadFailed')"
        :retry-label="t('common.retry')"
        @retry="stackStore.load(true)"
      />

      <!-- Spending overview. Failure keeps the item list visible and never
           shows a fake zero (UX-1); the failure is a compute problem, not data
           loss. -->
      <div class="card">
        <div class="card-header">
          <div>
            <p class="card-title">{{ t('stack.spendingTitle') }}</p>
            <p class="card-subtitle">{{ t('stack.hintMonthly') }} {{ t('stack.hintNoSum') }}</p>
          </div>
        </div>
        <div class="card-body stack-md">
          <template v-if="spendingError">
            <p class="hint">{{ t('stack.spendingErrorBody') }}</p>
            <ErrorBanner
              :message="spendingError"
              :title="t('stack.spendingErrorTitle')"
              :retry-label="t('stack.retrySpending')"
              @retry="stackStore.loadSpending(true)"
            />
          </template>
          <template v-else-if="spending">
            <div class="grid-cards" style="grid-template-columns: repeat(auto-fit, minmax(150px, 1fr))">
              <div class="surface-inset" style="padding: 9px">
                <p class="hint">{{ t('stack.statSubscriptions') }}</p>
                <p style="font-size: 18px; font-weight: 650">{{ spending.counts.total }}</p>
              </div>
              <div class="surface-inset" style="padding: 9px">
                <p class="hint">{{ t('stack.statUpcoming') }}</p>
                <p style="font-size: 18px; font-weight: 650">{{ spending.upcomingRenewals.length }}</p>
              </div>
              <div class="surface-inset" style="padding: 9px">
                <p class="hint">{{ t('stack.statUnknownRenewal') }}</p>
                <p style="font-size: 18px; font-weight: 650">{{ spending.renewalsUnknown.length }}</p>
              </div>
              <div class="surface-inset" style="padding: 9px">
                <p class="hint">{{ t('stack.statIncomplete') }}</p>
                <p style="font-size: 18px; font-weight: 650">{{ spending.incomplete.length }}</p>
              </div>
              <div class="surface-inset" style="padding: 9px">
                <p class="hint">{{ t('stack.statUsageNoBills') }}</p>
                <p style="font-size: 18px; font-weight: 650">{{ spending.usageWithoutBills.length }}</p>
              </div>
            </div>

            <p v-if="!hasSpendingData" class="hint">{{ t('stack.spendingEmpty') }}</p>
            <table v-else class="table">
              <thead>
                <tr>
                  <th>{{ t('stack.colCurrency') }}</th>
                  <th>{{ t('stack.colMonthly') }}</th>
                  <th>{{ t('stack.colAnnual') }}</th>
                  <th>{{ t('stack.colSubs') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="breakdown in spending.byCurrency" :key="breakdown.currency">
                  <td>{{ breakdown.currency }}</td>
                  <td>{{ currencyRow(breakdown).monthly }}</td>
                  <td>{{ currencyRow(breakdown).annual }}</td>
                  <td>{{ breakdown.subscriptionCount }}</td>
                </tr>
              </tbody>
            </table>

            <ul class="list-reset stack-sm">
              <li v-for="item in spending.upcomingRenewals" :key="item.id" class="surface-inset" style="padding: 7px 9px">
                <span class="text-sm">{{ t('stack.upcomingItem', { name: item.productName, date: item.renewsOn }) }}</span>
              </li>
              <li v-for="item in spending.staleRenewals" :key="item.id" class="surface-inset" style="padding: 7px 9px">
                <StatusBadge :label="t('stack.staleTitle')" tone="warn" />
                <span class="text-sm" style="margin-left: 5px">
                  {{ t('stack.staleItem', { name: item.productName, date: item.renewsOn }) }}
                </span>
              </li>
            </ul>

            <div v-if="spending.incomplete.length > 0" class="notice notice-info">
              <Icon class="notice-icon" name="info" :size="14" />
              <div class="notice-body">
                <p v-for="entry in spending.incomplete" :key="entry.subscription.id">
                  {{ entry.subscription.productName }}:
                  <span v-for="reason in entry.missing" :key="reason" class="chip" style="margin-right: 3px">
                    {{ t(`stack.missing.${reason}`) }}
                  </span>
                </p>
              </div>
            </div>
            <div v-if="spending.usageWithoutBills.length > 0" class="notice notice-info">
              <Icon class="notice-icon" name="info" :size="14" />
              <div class="notice-body">
                <p v-for="item in spending.usageWithoutBills" :key="item.id">
                  {{ item.productName }}: {{ t('stack.missing.usage_not_foldable') }}
                </p>
              </div>
            </div>
          </template>
          <p v-else class="hint">{{ t('stack.spendingEmpty') }}</p>
        </div>
      </div>

      <LoadingState v-if="loading && items.length === 0" :label="t('common.loadingData')" />

      <EmptyState
        v-else-if="items.length === 0"
        icon="stack"
        :title="t('stack.emptyTitle')"
        :body="t('stack.emptyBody')"
      >
        <template #actions>
          <button type="button" class="btn btn-primary" :disabled="refreshing" @click="refreshDetections">
            <Icon name="refresh" :size="13" />
            {{ t('stack.refresh') }}
          </button>
          <button type="button" class="btn" @click="openEditor()">
            <Icon name="plus" :size="13" />
            {{ t('stack.addItem') }}
          </button>
        </template>
      </EmptyState>

      <div v-else class="grid-cards">
        <article v-for="item in items" :key="item.id" class="card">
          <div class="card-body stack-md">
            <div class="row row-wrap" style="justify-content: space-between; align-items: flex-start">
              <div style="min-width: 0">
                <div class="row row-wrap" style="gap: 5px">
                  <h3 style="font-size: 13px; font-weight: 650">{{ item.name }}</h3>
                  <span class="badge badge-outline">{{ t(stackSourceKey[item.source]) }}</span>
                  <span class="badge badge-neutral">{{ t(stackDetectionStateKey[item.detectionState]) }}</span>
                  <span class="badge badge-neutral">{{ t(stackSubscriptionStateKey[item.subscriptionState]) }}</span>
                  <StatusBadge
                    :label="item.confirmed ? t('stack.badgeConfirmed') : t('stack.badgeUnconfirmed')"
                    :tone="item.confirmed ? 'success' : 'neutral'"
                  />
                </div>
                <p class="hint" style="margin-top: 3px">{{ item.vendor || '—' }}</p>
              </div>
            </div>

            <div class="row row-wrap" style="gap: 4px">
              <button type="button" class="btn btn-sm" @click="openDetail(item)">
                <Icon name="eye" :size="11" />
                {{ t('stack.actionDetail') }}
              </button>
              <button
                v-if="!item.confirmed"
                type="button"
                class="btn btn-sm"
                @click="confirmItem(item)"
              >
                <Icon name="check" :size="11" />
                {{ t('stack.actionConfirm') }}
              </button>
              <button type="button" class="btn btn-sm btn-ghost" @click="openEditor(item)">
                <Icon name="edit" :size="11" />
                {{ t('stack.actionEdit') }}
              </button>
              <button
                v-if="!item.mergedInto"
                type="button"
                class="btn btn-sm btn-ghost"
                @click="openMerge(item)"
              >
                <Icon name="link" :size="11" />
                {{ t('stack.actionMerge') }}
              </button>
              <button
                v-if="item.mergedInto"
                type="button"
                class="btn btn-sm btn-ghost"
                @click="unmergeTarget = item"
              >
                <Icon name="rollback" :size="11" />
                {{ t('stack.actionUnmerge') }}
              </button>
              <button type="button" class="btn btn-sm btn-ghost" @click="deleteTarget = item">
                <Icon name="trash" :size="11" />
                {{ t('stack.actionDelete') }}
              </button>
            </div>
            <p v-if="item.mergedInto" class="hint">
              <span class="chip">{{ t('stack.mergedChip', { name: item.mergedInto }) }}</span>
            </p>
          </div>
        </article>
      </div>
    </div>

    <BaseModal
      v-if="editorOpen"
      :title="editingId ? t('stack.editorTitleEdit') : t('stack.editorTitleCreate')"
      size="md"
      @close="editorOpen = false"
    >
      <div class="stack-md">
        <div class="field">
          <label class="label" for="stack-name">{{ t('stack.fieldName') }}</label>
          <input id="stack-name" v-model="draft.name" class="input" type="text" />
        </div>
        <div class="field">
          <label class="label" for="stack-vendor">{{ t('stack.fieldVendor') }}</label>
          <input id="stack-vendor" v-model="draft.vendor" class="input" type="text" />
        </div>
        <div class="field">
          <label class="label" for="stack-overlap">{{ t('stack.fieldOverlapGroup') }}</label>
          <input id="stack-overlap" v-model="draft.overlapGroup" class="input" type="text" />
          <p class="hint">{{ t('stack.fieldOverlapGroupHint') }}</p>
        </div>
        <div class="field">
          <label class="label" for="stack-notes">{{ t('stack.fieldNotes') }}</label>
          <input id="stack-notes" v-model="draft.notes" class="input" type="text" />
        </div>
        <ErrorBanner v-if="editorError" :message="editorError" />
      </div>
      <template #footer>
        <button type="button" class="btn" :disabled="saving" @click="editorOpen = false">
          {{ t('common.cancel') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="saving" @click="save">
          <span v-if="saving" class="spinner spinner-sm" aria-hidden="true" />
          {{ t('common.save') }}
        </button>
      </template>
    </BaseModal>

    <BaseModal
      v-if="mergeTarget"
      :title="t('stack.mergeTitle')"
      size="md"
      @close="mergeTarget = null"
    >
      <div class="stack-md">
        <p>{{ t('stack.mergePick') }}</p>
        <div v-if="mergeCandidates.length === 0" class="hint">{{ t('stack.mergeNone') }}</div>
        <label v-for="candidate in mergeCandidates" :key="candidate.id" class="checkbox-row">
          <input v-model="mergeIntoId" type="radio" :value="candidate.id" />
          <span>{{ candidate.name }}</span>
        </label>
        <ErrorBanner v-if="mergeError" :message="mergeError" />
      </div>
      <template #footer>
        <button type="button" class="btn" @click="mergeTarget = null">{{ t('common.cancel') }}</button>
        <button
          type="button"
          class="btn btn-primary"
          :disabled="merging || !mergeIntoId"
          @click="doMerge"
        >
          {{ t('stack.mergeInto') }}
        </button>
      </template>
    </BaseModal>

    <ConfirmDialog
      v-if="unmergeTarget"
      :title="t('stack.unmergeTitle')"
      :message="t('stack.unmergeBody', { name: unmergeTarget.name })"
      :confirm-label="t('common.confirm')"
      :busy="unmerging"
      @cancel="unmergeTarget = null"
      @confirm="doUnmerge"
    />

    <ConfirmDialog
      v-if="deleteTarget"
      :title="t('stack.deleteTitle')"
      :message="t('stack.deleteBody', { name: deleteTarget.name })"
      :confirm-label="t('stack.deleteConfirm')"
      danger
      :busy="deleting"
      @cancel="deleteTarget = null"
      @confirm="doDelete"
    />
    <ErrorBanner v-if="deleteError" :message="deleteError" :title="t('stack.errors.deleteFailed')" />
  </div>
</template>
