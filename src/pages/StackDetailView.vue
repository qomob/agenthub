<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import BaseModal from '../components/BaseModal.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import EmptyState from '../components/EmptyState.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { api, ApiError, normalizeError } from '../lib/api'
import { formatDateTime } from '../lib/format'
import {
  billingTypeKey,
  billingPeriodKey,
  formatMoneyMinor,
  missingReasonKey,
  parseMoneyMajor,
  stackDetectionStateKey,
  stackSourceKey,
  subscriptionStatusKey,
} from '../lib/stack'
import type {
  BillingPeriod,
  BillingType,
  SpendingOverview,
  StackItemDetail,
  StackItemView,
  SubscriptionStatus,
  SubscriptionView,
} from '../lib/types'
import { useStackStore } from '../stores/stack'
import { useUiStore } from '../stores/ui'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const stackStore = useStackStore()
const ui = useUiStore()

const itemId = computed(() => String(route.params.id ?? ''))
const detail = ref<StackItemDetail | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)

// Missing-cost reasons come from the core spending overview (never derived in
// the UI): subscriptionId → missing[].
const missingBySubscription = ref<Record<string, string[]>>({})

const unmergeTarget = ref(false)
const unmerging = ref(false)
const itemError = ref<string | null>(null)

// -- subscription editor ----------------------------------------------------

const BILLING_TYPES: BillingType[] = ['free', 'subscription', 'usage', 'hybrid', 'unknown']
const BILLING_PERIODS: BillingPeriod[] = ['monthly', 'quarterly', 'semiannual', 'yearly', 'usage', 'unknown']
const SUBSCRIPTION_STATUSES: SubscriptionStatus[] = ['active', 'paused', 'cancelled', 'expired', 'unknown']

const subEditorOpen = ref(false)
const editingSubId = ref<string | null>(null)
const savingSub = ref(false)
const subError = ref<string | null>(null)
const subDraft = reactive({
  productName: '',
  vendor: '',
  planName: '',
  billingType: 'subscription' as BillingType,
  amount: '',
  currency: '',
  billingPeriod: 'unknown' as BillingPeriod,
  startedOn: '',
  renewsOn: '',
  status: 'active' as SubscriptionStatus,
  notes: '',
  infoSource: '',
})

const subDeleteTarget = ref<SubscriptionView | null>(null)
const deletingSub = ref(false)

const subscriptions = computed(() => detail.value?.subscriptions ?? [])

onMounted(load)
watch(itemId, load)

async function load(): Promise<void> {
  if (!itemId.value) return
  loading.value = true
  error.value = null
  try {
    detail.value = await api.stackGet(itemId.value)
    // Core-computed missing facts for the incomplete badge (no UI rules).
    const overview = await stackStore.loadSpending()
    missingBySubscription.value = Object.fromEntries(
      ((overview as SpendingOverview | null)?.incomplete ?? []).map((entry) => [
        entry.subscription.id,
        entry.missing,
      ]),
    )
  } catch (err) {
    error.value = normalizeError(err, t('stack.errors.loadFailed'))
  } finally {
    loading.value = false
  }
}

function back(): void {
  void router.push({ name: 'stack' })
}

function formatAmount(subscription: SubscriptionView): string {
  return formatMoneyMinor(subscription.amountMinor, subscription.currency) ?? t('stack.amountUnknown')
}

function openSubEditor(subscription?: SubscriptionView): void {
  editingSubId.value = subscription?.id ?? null
  subDraft.productName = subscription?.productName ?? ''
  subDraft.vendor = subscription?.vendor ?? ''
  subDraft.planName = subscription?.planName ?? ''
  subDraft.billingType = subscription?.billingType ?? 'subscription'
  // Integer-safe major text for editing; reuse the display formatter and drop
  // the currency suffix (never float division).
  subDraft.amount =
    subscription?.amountMinor !== undefined && subscription?.currency
      ? (formatMoneyMinor(subscription.amountMinor, subscription.currency) ?? '').split(' ')[0]
      : ''
  subDraft.currency = subscription?.currency ?? ''
  subDraft.billingPeriod = subscription?.billingPeriod ?? 'unknown'
  subDraft.startedOn = subscription?.startedOn ?? ''
  subDraft.renewsOn = subscription?.renewsOn ?? ''
  subDraft.status = subscription?.status ?? 'active'
  subDraft.notes = subscription?.notes ?? ''
  subDraft.infoSource = subscription?.infoSource ?? ''
  subError.value = null
  subEditorOpen.value = true
}

async function saveSubscription(): Promise<void> {
  if (!subDraft.productName.trim()) {
    subError.value = t('stack.subProductRequired')
    return
  }
  const currency = subDraft.currency.trim().toUpperCase() || undefined
  const amountMinor = parseMoneyMajor(subDraft.amount, currency)
  if (amountMinor === null) {
    subError.value = currency ? t('stack.subAmountInvalid') : t('stack.subAmountCurrencyRequired')
    return
  }
  if (amountMinor !== undefined && !currency) {
    subError.value = t('stack.subAmountCurrencyRequired')
    return
  }
  if (currency && !/^[A-Z]{3}$/.test(currency)) {
    subError.value = t('stack.subCurrencyInvalid')
    return
  }
  savingSub.value = true
  subError.value = null
  try {
    await api.subscriptionUpsert({
      id: editingSubId.value ?? undefined,
      stackItemId: itemId.value,
      productName: subDraft.productName.trim(),
      vendor: subDraft.vendor.trim() || undefined,
      planName: subDraft.planName.trim() || undefined,
      billingType: subDraft.billingType,
      amountMinor,
      currency,
      billingPeriod: subDraft.billingPeriod,
      startedOn: subDraft.startedOn.trim() || undefined,
      renewsOn: subDraft.renewsOn.trim() || undefined,
      status: subDraft.status,
      notes: subDraft.notes.trim() || undefined,
      infoSource: subDraft.infoSource.trim() || undefined,
    })
    subEditorOpen.value = false
    ui.notify('success', t('stack.subSaved'))
    await load()
  } catch (err) {
    subError.value = normalizeError(err, t('stack.errors.saveFailed'))
  } finally {
    savingSub.value = false
  }
}

async function deleteSubscription(): Promise<void> {
  const subscription = subDeleteTarget.value
  if (!subscription) return
  deletingSub.value = true
  try {
    await api.subscriptionDelete(subscription.id)
    subDeleteTarget.value = null
    ui.notify('success', t('stack.subDeleted'))
    await load()
  } catch (err) {
    subDeleteTarget.value = null
    ui.notifyError(t('stack.errors.deleteFailed'), normalizeError(err))
  } finally {
    deletingSub.value = false
  }
}

async function doUnmerge(): Promise<void> {
  unmerging.value = true
  itemError.value = null
  try {
    await api.stackUnmergeItem(itemId.value)
    unmergeTarget.value = false
    ui.notify('success', t('stack.unmergeDone'))
    await load()
    await stackStore.load(true)
  } catch (err) {
    itemError.value = normalize(err)
  } finally {
    unmerging.value = false
  }
}

// -- item confirm / edit / delete -------------------------------------------

const confirmingItem = ref(false)
const itemEditorOpen = ref(false)
const itemSaving = ref(false)
const itemEditorError = ref<string | null>(null)
const itemDraft = reactive({ name: '', vendor: '', overlapGroup: '', notes: '' })
const deleteItemOpen = ref(false)
const deletingItem = ref(false)

async function confirmItem(): Promise<void> {
  confirmingItem.value = true
  itemError.value = null
  try {
    await api.stackConfirmItem(itemId.value)
    ui.notify('success', t('stack.badgeConfirmed'))
    await load()
  } catch (err) {
    itemError.value = normalize(err)
  } finally {
    confirmingItem.value = false
  }
}

function openItemEditor(): void {
  const item = detail.value?.item
  if (!item) return
  itemDraft.name = item.name
  itemDraft.vendor = item.vendor ?? ''
  itemDraft.overlapGroup = item.overlapGroup ?? ''
  itemDraft.notes = item.notes ?? ''
  itemEditorError.value = null
  itemEditorOpen.value = true
}

async function saveItem(): Promise<void> {
  if (!itemDraft.name.trim()) {
    itemEditorError.value = t('stack.fieldNameRequired')
    return
  }
  itemSaving.value = true
  itemEditorError.value = null
  try {
    await api.stackUpsertItem({
      id: itemId.value,
      name: itemDraft.name.trim(),
      vendor: itemDraft.vendor.trim() || undefined,
      overlapGroup: itemDraft.overlapGroup.trim() || undefined,
      notes: itemDraft.notes.trim() || undefined,
    })
    itemEditorOpen.value = false
    ui.notify('success', t('stack.saved'))
    await load()
    await stackStore.load(true)
  } catch (err) {
    itemEditorError.value = normalize(err)
  } finally {
    itemSaving.value = false
  }
}

async function deleteItem(): Promise<void> {
  deletingItem.value = true
  itemError.value = null
  try {
    await api.stackDeleteItem(itemId.value)
    deleteItemOpen.value = false
    ui.notify('success', t('stack.deleted'))
    await stackStore.load(true)
    await router.push({ name: 'stack' })
  } catch (err) {
    itemError.value = normalize(err)
    deleteItemOpen.value = false
  } finally {
    deletingItem.value = false
  }
}

// -- merge ------------------------------------------------------------------

const mergeModalOpen = ref(false)
const mergeIntoId = ref('')
const mergeConfirmOpen = ref(false)
const merging = ref(false)
const mergeError = ref<string | null>(null)
const mergeCandidates = ref<StackItemView[]>([])

const mergeIntoName = computed(
  () =>
    mergeCandidates.value.find((candidate) => candidate.id === mergeIntoId.value)?.name ??
    mergeIntoId.value,
)

async function openMergeModal(): Promise<void> {
  mergeIntoId.value = ''
  mergeError.value = null
  try {
    const all = await api.stackList()
    // Already-merged items can never be merge targets (hard rule); neither can
    // the item itself.
    mergeCandidates.value = all.filter(
      (candidate) => candidate.id !== itemId.value && !candidate.mergedInto,
    )
    mergeModalOpen.value = true
  } catch (err) {
    itemError.value = normalize(err)
  }
}

async function doMerge(): Promise<void> {
  if (!mergeIntoId.value) return
  merging.value = true
  mergeError.value = null
  try {
    await api.stackMergeItems(itemId.value, mergeIntoId.value)
    mergeModalOpen.value = false
    mergeConfirmOpen.value = false
    ui.notify('success', t('stack.mergeDone'))
    await load()
    await stackStore.load(true)
  } catch (err) {
    mergeError.value = normalize(err)
    mergeConfirmOpen.value = false
  } finally {
    merging.value = false
  }
}

function normalize(err: unknown): string {
  const code = err instanceof ApiError ? err.code : null
  if (code === 'RESOURCE_CONFLICT') {
    return t('stack.errors.resourceConflict')
  }
  return normalizeError(err, err instanceof Error ? err.message : String(err))
}
</script>

<template>
  <div class="page">
    <PageHeader
      :title="detail?.item.name ?? t('stack.title')"
      :subtitle="detail?.item.vendor || t('stack.subtitle')"
    >
      <template #meta>
        <div v-if="detail" class="row row-wrap" style="margin-top: 7px; gap: 5px">
          <button type="button" class="btn btn-ghost btn-sm" @click="back">
            <Icon name="arrow-left" :size="12" />
            {{ t('stack.backLink') }}
          </button>
          <span class="badge badge-outline">{{ t(stackSourceKey[detail.item.source]) }}</span>
          <span class="badge badge-neutral">{{ t(stackDetectionStateKey[detail.item.detectionState]) }}</span>
          <StatusBadge
            :label="detail.item.confirmed ? t('stack.badgeConfirmed') : t('stack.badgeUnconfirmed')"
            :tone="detail.item.confirmed ? 'success' : 'neutral'"
          />
          <span
            v-if="detail.item.mergedInto"
            class="chip"
          >{{ t('stack.mergedChip', { name: detail.item.mergedInto }) }}</span>
        </div>
      </template>
      <template #actions>
        <button type="button" class="btn" :disabled="loading" @click="load">
          <span v-if="loading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('common.refresh') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="!detail" @click="openSubEditor()">
          <Icon name="plus" :size="13" />
          {{ t('stack.addSubscription') }}
        </button>
        <button
          v-if="detail?.item.mergedInto"
          type="button"
          class="btn"
          @click="unmergeTarget = true"
        >
          <Icon name="rollback" :size="13" />
          {{ t('stack.actionUnmerge') }}
        </button>
        <button
          v-if="detail && !detail.item.confirmed"
          type="button"
          class="btn"
          :disabled="confirmingItem"
          @click="confirmItem"
        >
          <Icon name="check" :size="13" />
          {{ t('stack.actionConfirm') }}
        </button>
        <button type="button" class="btn" :disabled="!detail" @click="openItemEditor">
          <Icon name="edit" :size="13" />
          {{ t('stack.actionEdit') }}
        </button>
        <button
          v-if="detail && !detail.item.mergedInto"
          type="button"
          class="btn"
          :disabled="loading"
          @click="openMergeModal"
        >
          <Icon name="link" :size="13" />
          {{ t('stack.mergeInto') }}
        </button>
        <button
          v-if="detail"
          type="button"
          class="btn"
          :disabled="deletingItem"
          @click="deleteItemOpen = true"
        >
          <Icon name="trash" :size="13" />
          {{ t('common.delete') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner
        v-if="error"
        :message="error"
        :title="t('stack.errors.loadFailed')"
        :retry-label="t('common.retry')"
        @retry="load"
      />
      <ErrorBanner v-if="itemError" :message="itemError" />

      <LoadingState v-if="loading && !detail" :label="t('common.loadingData')" />

      <template v-else-if="detail">
        <div class="card">
          <div class="card-body stack-md">
            <div class="row row-wrap" style="gap: 18px">
              <div class="field" style="min-width: 180px">
                <span class="label">{{ t('stack.vendorLabel') }}</span>
                <span>{{ detail.item.vendor || '—' }}</span>
              </div>
              <div class="field" style="min-width: 180px">
                <span class="label">{{ t('stack.overlapGroupLabel') }}</span>
                <span>{{ detail.item.overlapGroup || '—' }}</span>
              </div>
              <div class="field" style="min-width: 180px">
                <span class="label">{{ t('stack.notesLabel') }}</span>
                <span>{{ detail.item.notes || '—' }}</span>
              </div>
            </div>
            <p class="hint">
              {{ t('stack.createdAtLabel') }}：{{ formatDateTime(detail.item.createdAt, 'zh-CN') }}
              <template v-if="detail.item.confirmed">
                · {{ t('stack.confirmedAtLabel') }}：{{ formatDateTime(detail.item.lastConfirmedAt, 'zh-CN') }}
              </template>
            </p>
          </div>
        </div>

        <div class="card">
          <div class="card-header">
            <p class="card-title">{{ t('stack.detectionsTitle') }}</p>
          </div>
          <div class="card-body">
            <p v-if="detail.detections.length === 0" class="hint">{{ t('stack.noDetections') }}</p>
            <ul v-else class="list-reset stack-sm">
              <li v-for="detection in detail.detections" :key="detection.id" class="surface-inset" style="padding: 8px 10px">
                <div class="row row-wrap" style="gap: 6px">
                  <span class="text-sm" style="font-weight: 550">{{ t('stack.adapterLabel') }}: {{ detection.adapterKey }}</span>
                  <span class="badge badge-neutral">{{ detection.detectedStatus }}</span>
                  <span class="hint">{{ formatDateTime(detection.detectedAt, 'zh-CN') }}</span>
                </div>
                <p v-for="evidence in detection.evidence" :key="evidence" class="hint mono" style="margin-top: 2px">
                  {{ t('stack.evidenceLabel') }}: {{ evidence }}
                </p>
              </li>
            </ul>
          </div>
        </div>

        <div class="card">
          <div class="card-header">
            <p class="card-title">{{ t('stack.subscriptionsTitle') }}</p>
          </div>
          <div class="card-body">
            <EmptyState
              v-if="subscriptions.length === 0"
              icon="key"
              :title="t('stack.subsEmptyTitle')"
              :body="t('stack.subsEmptyBody')"
            >
              <template #actions>
                <button type="button" class="btn btn-primary" @click="openSubEditor()">
                  <Icon name="plus" :size="13" />
                  {{ t('stack.addSubscription') }}
                </button>
              </template>
            </EmptyState>
            <ul v-else class="list-reset stack-sm">
              <li v-for="subscription in subscriptions" :key="subscription.id" class="surface-inset" style="padding: 9px 11px">
                <div class="row row-wrap" style="justify-content: space-between; align-items: flex-start">
                  <div style="min-width: 0">
                    <div class="row row-wrap" style="gap: 5px">
                      <span class="text-sm" style="font-weight: 550">{{ subscription.productName }}</span>
                      <span class="badge badge-outline">{{ t(billingTypeKey[subscription.billingType]) }}</span>
                      <StatusBadge
                        :label="t(subscriptionStatusKey[subscription.status])"
                        :tone="subscription.status === 'active' ? 'success' : 'neutral'"
                      />
                      <span v-if="missingBySubscription[subscription.id]" class="badge badge-accent">
                        {{ t('stack.incompleteBadge') }}
                      </span>
                    </div>
                    <div class="row row-wrap" style="gap: 12px; margin-top: 3px">
                      <span class="text-sm">{{ formatAmount(subscription) }}</span>
                      <span class="hint">{{ t('stack.periodLabel') }}: {{ t(billingPeriodKey[subscription.billingPeriod ?? 'unknown']) }}</span>
                      <span v-if="subscription.planName" class="hint">{{ t('stack.planLabel') }}: {{ subscription.planName }}</span>
                      <span v-if="subscription.startedOn" class="hint">{{ t('stack.startedOnLabel') }}: {{ subscription.startedOn }}</span>
                      <span v-if="subscription.renewsOn" class="hint">{{ t('stack.renewsOnLabel') }}: {{ subscription.renewsOn }}</span>
                    </div>
                    <p v-if="missingBySubscription[subscription.id]" class="hint" style="margin-top: 3px">
                      <span
                        v-for="reason in missingBySubscription[subscription.id]"
                        :key="reason"
                        class="chip"
                        style="margin-right: 3px"
                      >{{ t(missingReasonKey[reason] ?? reason) }}</span>
                    </p>
                    <p v-if="subscription.infoSource" class="hint" style="margin-top: 3px">
                      {{ t('stack.infoSourceLabel') }}: {{ subscription.infoSource }}
                    </p>
                    <p v-if="subscription.notes" class="hint" style="margin-top: 3px">
                      {{ t('stack.notesLabel') }}: {{ subscription.notes }}
                    </p>
                    <p v-if="subscription.lastConfirmedAt" class="hint" style="margin-top: 3px">
                      {{ t('stack.confirmedAtLabel') }}: {{ formatDateTime(subscription.lastConfirmedAt, 'zh-CN') }}
                    </p>
                  </div>
                  <div class="row">
                    <button type="button" class="btn btn-sm btn-ghost" @click="openSubEditor(subscription)">
                      <Icon name="edit" :size="11" />
                      {{ t('stack.actionEdit') }}
                    </button>
                    <button type="button" class="btn btn-sm btn-ghost" @click="subDeleteTarget = subscription">
                      <Icon name="trash" :size="11" />
                      {{ t('common.delete') }}
                    </button>
                  </div>
                </div>
              </li>
            </ul>
          </div>
        </div>

        <div v-if="detail.mergedChildren.length > 0" class="card">
          <div class="card-header">
            <p class="card-title">{{ t('stack.mergedChildrenTitle') }}</p>
          </div>
          <div class="card-body">
            <ul class="list-reset stack-sm">
              <li v-for="child in detail.mergedChildren" :key="child.id" class="surface-inset" style="padding: 7px 9px">
                <span class="text-sm">{{ child.name }}</span>
                <span class="badge badge-neutral" style="margin-left: 5px">{{ child.subscriptionCount }} subs</span>
              </li>
            </ul>
          </div>
        </div>
      </template>
    </div>

    <BaseModal
      v-if="subEditorOpen"
      :title="editingSubId ? t('stack.subEditorEdit') : t('stack.subEditorCreate')"
      size="md"
      @close="subEditorOpen = false"
    >
      <div class="stack-md">
        <div class="field">
          <label class="label" for="sub-product">{{ t('stack.subProduct') }}</label>
          <input id="sub-product" v-model="subDraft.productName" class="input" type="text" />
        </div>
        <div class="row" style="gap: 12px">
          <div class="field" style="flex: 1">
            <label class="label" for="sub-vendor">{{ t('stack.subVendor') }}</label>
            <input id="sub-vendor" v-model="subDraft.vendor" class="input" type="text" />
          </div>
          <div class="field" style="flex: 1">
            <label class="label" for="sub-plan">{{ t('stack.subPlanName') }}</label>
            <input id="sub-plan" v-model="subDraft.planName" class="input" type="text" />
          </div>
        </div>
        <div class="row" style="gap: 12px">
          <div class="field" style="flex: 1">
            <label class="label" for="sub-billing-type">{{ t('stack.subBillingType') }}</label>
            <select id="sub-billing-type" v-model="subDraft.billingType" class="input">
              <option v-for="option in BILLING_TYPES" :key="option" :value="option">
                {{ t(billingTypeKey[option]) }}
              </option>
            </select>
          </div>
          <div class="field" style="flex: 1">
            <label class="label" for="sub-status">{{ t('stack.subStatus') }}</label>
            <select id="sub-status" v-model="subDraft.status" class="input">
              <option v-for="option in SUBSCRIPTION_STATUSES" :key="option" :value="option">
                {{ t(subscriptionStatusKey[option]) }}
              </option>
            </select>
          </div>
        </div>
        <div class="row" style="gap: 12px">
          <div class="field" style="flex: 1">
            <label class="label" for="sub-amount">{{ t('stack.subAmount') }}</label>
            <input id="sub-amount" v-model="subDraft.amount" class="input" type="text" inputmode="decimal" />
            <p class="hint">{{ t('stack.subAmountHint') }}</p>
          </div>
          <div class="field" style="flex: 1">
            <label class="label" for="sub-currency">{{ t('stack.subCurrency') }}</label>
            <input
              id="sub-currency"
              v-model="subDraft.currency"
              class="input mono"
              type="text"
              maxlength="3"
              style="text-transform: uppercase"
            />
            <p class="hint">{{ t('stack.subCurrencyHint') }}</p>
          </div>
          <div class="field" style="flex: 1">
            <label class="label" for="sub-period">{{ t('stack.subBillingPeriod') }}</label>
            <select id="sub-period" v-model="subDraft.billingPeriod" class="input">
              <option v-for="option in BILLING_PERIODS" :key="option" :value="option">
                {{ option === 'unknown' ? t('stack.subPeriodUnknownOption') : t(billingPeriodKey[option]) }}
              </option>
            </select>
          </div>
        </div>
        <p v-if="subDraft.billingType === 'free'" class="hint">{{ t('stack.freeHint') }}</p>
        <p v-if="subDraft.billingType === 'usage'" class="hint">{{ t('stack.usageHint') }}</p>
        <div class="row" style="gap: 12px">
          <div class="field" style="flex: 1">
            <label class="label" for="sub-started">{{ t('stack.subStartedOn') }}</label>
            <input id="sub-started" v-model="subDraft.startedOn" class="input" type="date" />
          </div>
          <div class="field" style="flex: 1">
            <label class="label" for="sub-renews">{{ t('stack.subRenewsOn') }}</label>
            <input id="sub-renews" v-model="subDraft.renewsOn" class="input" type="date" />
          </div>
        </div>
        <div class="row" style="gap: 12px">
          <div class="field" style="flex: 1">
            <label class="label" for="sub-notes">{{ t('stack.subNotes') }}</label>
            <input id="sub-notes" v-model="subDraft.notes" class="input" type="text" />
          </div>
          <div class="field" style="flex: 1">
            <label class="label" for="sub-source">{{ t('stack.subInfoSource') }}</label>
            <input id="sub-source" v-model="subDraft.infoSource" class="input" type="text" />
          </div>
        </div>
        <ErrorBanner v-if="subError" :message="subError" />
      </div>
      <template #footer>
        <button type="button" class="btn" :disabled="savingSub" @click="subEditorOpen = false">
          {{ t('common.cancel') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="savingSub" @click="saveSubscription">
          <span v-if="savingSub" class="spinner spinner-sm" aria-hidden="true" />
          {{ t('common.save') }}
        </button>
      </template>
    </BaseModal>

    <ConfirmDialog
      v-if="subDeleteTarget"
      :title="t('stack.subDeleteTitle')"
      :message="t('stack.subDeleteBody', { name: subDeleteTarget.productName })"
      :confirm-label="t('common.delete')"
      danger
      :busy="deletingSub"
      @cancel="subDeleteTarget = null"
      @confirm="deleteSubscription"
    />

    <ConfirmDialog
      v-if="unmergeTarget && detail"
      :title="t('stack.unmergeTitle')"
      :message="t('stack.unmergeBody', { name: detail.item.name })"
      :confirm-label="t('common.confirm')"
      :busy="unmerging"
      @cancel="unmergeTarget = false"
      @confirm="doUnmerge"
    />

    <BaseModal
      v-if="itemEditorOpen"
      :title="t('stack.editorTitleEdit')"
      size="md"
      @close="itemEditorOpen = false"
    >
      <div class="stack-md">
        <div class="field">
          <label class="label" for="detail-item-name">{{ t('stack.fieldName') }}</label>
          <input id="detail-item-name" v-model="itemDraft.name" class="input" type="text" />
        </div>
        <div class="field">
          <label class="label" for="detail-item-vendor">{{ t('stack.fieldVendor') }}</label>
          <input id="detail-item-vendor" v-model="itemDraft.vendor" class="input" type="text" />
        </div>
        <div class="field">
          <label class="label" for="detail-item-overlap">{{ t('stack.fieldOverlapGroup') }}</label>
          <input id="detail-item-overlap" v-model="itemDraft.overlapGroup" class="input" type="text" />
          <p class="hint">{{ t('stack.fieldOverlapGroupHint') }}</p>
        </div>
        <div class="field">
          <label class="label" for="detail-item-notes">{{ t('stack.fieldNotes') }}</label>
          <input id="detail-item-notes" v-model="itemDraft.notes" class="input" type="text" />
        </div>
        <ErrorBanner v-if="itemEditorError" :message="itemEditorError" />
      </div>
      <template #footer>
        <button type="button" class="btn" :disabled="itemSaving" @click="itemEditorOpen = false">
          {{ t('common.cancel') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="itemSaving" @click="saveItem">
          <span v-if="itemSaving" class="spinner spinner-sm" aria-hidden="true" />
          {{ t('common.save') }}
        </button>
      </template>
    </BaseModal>

    <BaseModal
      v-if="mergeModalOpen"
      :title="t('stack.mergeTitle')"
      size="md"
      @close="mergeModalOpen = false"
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
        <button type="button" class="btn" @click="mergeModalOpen = false">
          {{ t('common.cancel') }}
        </button>
        <button
          type="button"
          class="btn btn-primary"
          :disabled="merging || !mergeIntoId"
          @click="mergeConfirmOpen = true"
        >
          {{ t('stack.mergeInto') }}
        </button>
      </template>
    </BaseModal>

    <ConfirmDialog
      v-if="mergeConfirmOpen && mergeIntoId"
      :title="t('stack.mergeConfirmTitle')"
      :message="t('stack.mergeConfirmBody', { child: detail?.item.name, into: mergeIntoName })"
      :confirm-label="t('common.confirm')"
      :busy="merging"
      @cancel="mergeConfirmOpen = false"
      @confirm="doMerge"
    />

    <ConfirmDialog
      v-if="deleteItemOpen && detail"
      :title="t('stack.deleteTitle')"
      :message="t('stack.deleteBody', { name: detail.item.name })"
      :confirm-label="t('stack.deleteConfirm')"
      danger
      :busy="deletingItem"
      @cancel="deleteItemOpen = false"
      @confirm="deleteItem"
    />
  </div>
</template>
