<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import BaseModal from '../components/BaseModal.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import HealthStatusBadge from '../components/HealthStatusBadge.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import StatusBadge from '../components/StatusBadge.vue'
import UpdateImpactSummary from '../components/UpdateImpactSummary.vue'
import { api, normalizeError } from '../lib/api'
import { formatDateTime, formatRelativeTime } from '../lib/format'
import {
  auditRiskKey,
  auditRiskTone,
  auditStatusKey,
  auditStatusTone,
  dependencyKindKey,
  dependencyStatusKey,
  dependencyStatusTone,
  environmentStatusKey,
  environmentStatusTone,
  findingCategoryKey,
  findingSeverityKey,
  toneFor,
  validationKindKey,
  validationStatusKey,
  validationStatusTone,
} from '../lib/health'
import type {
  CapabilityAudit,
  CapabilityHealth,
  CapabilityVersion,
  DependencyReport,
  InstallationView,
  Skill,
  UpdateImpact,
  ValidationRun,
  ValidationStatus,
} from '../lib/types'
import { useUiStore } from '../stores/ui'

const props = defineProps<{
  skill: Skill
  versions: CapabilityVersion[]
}>()

const { t, locale } = useI18n()
const ui = useUiStore()

const health = ref<CapabilityHealth | null>(null)
const healthLoading = ref(false)
const healthError = ref<string | null>(null)

const audit = ref<CapabilityAudit | null>(null)
const auditBusy = ref(false)
const auditError = ref<string | null>(null)

const dependencies = ref<DependencyReport | null>(null)
const depsBusy = ref(false)
const depsError = ref<string | null>(null)

const impact = ref<UpdateImpact | null>(null)
const impactBusy = ref(false)
const impactError = ref<string | null>(null)
const impactTargetId = ref<string>('')

const validations = ref<ValidationRun[]>([])
const validationsLoading = ref(false)
const validationsError = ref<string | null>(null)

const recordOpen = ref(false)
const recordBusy = ref(false)
const recordError = ref<string | null>(null)
const recordForm = ref({
  agentId: '',
  caseName: '',
  status: 'passed' as ValidationStatus,
  notes: '',
  evidence: '',
})

const VALIDATION_STATUSES: ValidationStatus[] = [
  'passed',
  'failed',
  'needs_attention',
  'unverified',
]

const installedAgents = computed<InstallationView[]>(() => {
  const seen = new Map<string, InstallationView>()
  for (const installation of props.skill.installations) {
    if (!seen.has(installation.agentId)) seen.set(installation.agentId, installation)
  }
  return [...seen.values()]
})

const targetVersions = computed(() => props.versions)
const impactTargetIsCurrent = computed(
  () =>
    impactTargetId.value.length > 0 && impactTargetId.value === props.skill.currentVersionId,
)
const canAnalyzeImpact = computed(
  () => impactTargetId.value.length > 0 && !impactTargetIsCurrent.value && !impactBusy.value,
)

onMounted(() => {
  impactTargetId.value = props.skill.currentVersionId
  void load()
})

watch(
  () => props.skill.id,
  () => {
    impact.value = null
    impactError.value = null
    impactTargetId.value = props.skill.currentVersionId
    void load()
  },
)

watch(
  () => props.skill.currentVersionId,
  (value) => {
    if (!impactTargetId.value || impactTargetId.value === value) impactTargetId.value = value
  },
)

async function load(): Promise<void> {
  healthError.value = null
  validationsError.value = null
  healthLoading.value = true
  validationsLoading.value = true
  const [healthResult, validationsResult] = await Promise.allSettled([
    api.skillsHealth(props.skill.id),
    api.validationsList(props.skill.id, 50),
  ])
  if (healthResult.status === 'fulfilled') {
    health.value = healthResult.value
    audit.value = healthResult.value.audit
    dependencies.value = healthResult.value.environment
  } else {
    healthError.value = normalizeError(healthResult.reason, t('errors.healthFailed'))
  }
  if (validationsResult.status === 'fulfilled') {
    validations.value = validationsResult.value
  } else {
    validationsError.value = normalizeError(validationsResult.reason, t('errors.validationsFailed'))
  }
  healthLoading.value = false
  validationsLoading.value = false
}

async function refreshHealth(): Promise<void> {
  try {
    const next = await api.skillsHealth(props.skill.id)
    health.value = next
    audit.value = next.audit
    dependencies.value = next.environment
    healthError.value = null
  } catch (err) {
    healthError.value = normalizeError(err, t('errors.healthFailed'))
  }
}

async function runAudit(): Promise<void> {
  auditBusy.value = true
  auditError.value = null
  try {
    audit.value = await api.skillsAudit({
      capabilityId: props.skill.id,
      versionId: props.skill.currentVersionId || null,
      kind: 'static',
    })
    await refreshHealth()
    ui.notify('success', t('skillHealth.auditDone'))
  } catch (err) {
    auditError.value = normalizeError(err, t('errors.auditFailed'))
  } finally {
    auditBusy.value = false
  }
}

async function checkDependencies(): Promise<void> {
  depsBusy.value = true
  depsError.value = null
  try {
    dependencies.value = await api.skillsDependencies({
      capabilityId: props.skill.id,
      versionId: props.skill.currentVersionId || null,
      kind: null,
    })
    await refreshHealth()
    ui.notify('success', t('skillHealth.dependenciesDone'))
  } catch (err) {
    depsError.value = normalizeError(err, t('errors.dependenciesFailed'))
  } finally {
    depsBusy.value = false
  }
}

async function analyzeImpact(): Promise<void> {
  if (!canAnalyzeImpact.value) return
  impactBusy.value = true
  impactError.value = null
  try {
    impact.value = await api.skillsPreviewUpdate({
      capabilityId: props.skill.id,
      fromVersionId: props.skill.currentVersionId || null,
      toVersionId: impactTargetId.value,
    })
  } catch (err) {
    impactError.value = normalizeError(err, t('errors.impactFailed'))
  } finally {
    impactBusy.value = false
  }
}

function openRecord(): void {
  recordForm.value = {
    agentId: '',
    caseName: '',
    status: 'passed',
    notes: '',
    evidence: '',
  }
  recordError.value = null
  recordOpen.value = true
}

function closeRecord(): void {
  if (recordBusy.value) return
  recordOpen.value = false
}

async function submitRecord(): Promise<void> {
  if (recordBusy.value) return
  const caseName = recordForm.value.caseName.trim()
  if (!caseName) {
    recordError.value = t('skillHealth.caseRequired')
    return
  }
  recordBusy.value = true
  recordError.value = null
  try {
    const run = await api.validationsRecord({
      capabilityId: props.skill.id,
      versionId: props.skill.currentVersionId || null,
      agentId: recordForm.value.agentId || null,
      kind: recordForm.value.agentId ? 'manual' : 'static',
      status: recordForm.value.status,
      caseName,
      notes: recordForm.value.notes.trim() || null,
      evidence: recordForm.value.evidence.trim() || null,
    })
    validations.value = [run, ...validations.value]
    recordOpen.value = false
    await refreshHealth()
    ui.notify('success', t('skillHealth.validationRecorded'))
  } catch (err) {
    recordError.value = normalizeError(err, t('errors.validationRecordFailed'))
  } finally {
    recordBusy.value = false
  }
}

function auditStatusLabel(value: string): string {
  const key = auditStatusKey[value]
  return key ? t(key) : value
}

function riskLabel(value: string): string {
  const key = auditRiskKey[value]
  return key ? t(key) : value
}

function findingSeverityLabel(value: string): string {
  const key = findingSeverityKey[value]
  return key ? t(key) : value
}

function findingCategoryLabel(value: string): string {
  const key = findingCategoryKey[value]
  return key ? t(key) : value
}

function dependencyKindLabel(value: string): string {
  const key = dependencyKindKey[value]
  return key ? t(key) : value
}

function dependencyStatusLabel(value: string): string {
  const key = dependencyStatusKey[value]
  return key ? t(key) : value
}

function environmentStatusLabel(value: string): string {
  const key = environmentStatusKey[value]
  return key ? t(key) : value
}

function validationStatusLabel(value: string): string {
  const key = validationStatusKey[value]
  return key ? t(key) : value
}

function validationKindLabel(value: string): string {
  const key = validationKindKey[value]
  return key ? t(key) : value
}

function lastCheckedLabel(): string {
  if (!health.value?.lastCheckedAt) return t('health.neverChecked')
  return `${formatRelativeTime(health.value.lastCheckedAt, locale.value)} · ${formatDateTime(
    health.value.lastCheckedAt,
    locale.value,
  )}`
}
</script>

<template>
  <div class="section-stack">
    <ErrorBanner
      v-if="healthError"
      :message="healthError"
      :title="t('errors.healthFailed')"
      :retry-label="t('common.retry')"
      @retry="load"
    />

    <div v-if="healthLoading && !health" class="card">
      <div class="card-body">
        <LoadingState :label="t('skillHealth.loading')" compact />
      </div>
    </div>

    <!-- Top-of-page health summary (skills_health) -->
    <div v-if="health" class="card">
      <div class="card-header">
        <div>
          <p class="card-title">{{ t('skillHealth.title') }}</p>
          <p class="card-subtitle">
            {{ t('health.lastChecked') }}：{{ lastCheckedLabel() }}
          </p>
        </div>
        <div class="row row-wrap" style="gap: 5px">
          <HealthStatusBadge :state="health.status" />
          <button type="button" class="btn btn-sm" :disabled="healthLoading" @click="load">
            <span v-if="healthLoading" class="spinner spinner-sm" aria-hidden="true" />
            <Icon v-else name="refresh" :size="12" />
            {{ t('common.refresh') }}
          </button>
        </div>
      </div>
      <div class="card-body stack-sm">
        <p class="hint">{{ t('skillHealth.lead') }}</p>
        <div class="row row-wrap" style="gap: 5px">
          <span class="badge badge-neutral">
            {{ t('skillHealth.installedAgents', { count: health.installationCount }) }}
          </span>
          <span class="badge badge-success">
            {{ t('skillHealth.verifiedAgents', { count: health.verifiedAgentKeys.length }) }}
          </span>
          <StatusBadge
            :label="`${t('skillHealth.auditRisk')}：${health.audit ? riskLabel(health.audit.risk) : t('health.notAudited')}`"
            :tone="health.audit ? toneFor(auditRiskTone, health.audit.risk) : 'neutral'"
            :dot="false"
          />
          <StatusBadge
            :label="`${t('skillHealth.environment')}：${health.environment ? environmentStatusLabel(health.environment.status) : t('skillHealth.notChecked')}`"
            :tone="health.environment ? toneFor(environmentStatusTone, health.environment.status) : 'neutral'"
            :dot="false"
          />
        </div>
        <ul v-if="health.reasons.length > 0" class="list-reset stack-sm">
          <li v-for="reason in health.reasons" :key="reason" class="hint">• {{ reason }}</li>
        </ul>
        <p v-else class="hint">{{ t('skillHealth.noReasons') }}</p>
        <p v-if="health.verifiedAgentKeys.length > 0" class="hint">
          {{ t('skillHealth.verifiedAgentNames') }}：{{ health.verifiedAgentKeys.join(' · ') }}
        </p>
      </div>
    </div>

    <div class="grid-2">
      <div class="section-stack">
        <!-- Security audit -->
        <div class="card">
          <div class="card-header">
            <div>
              <p class="card-title">{{ t('skillHealth.auditTitle') }}</p>
              <p class="card-subtitle">{{ t('skillHealth.auditLead') }}</p>
            </div>
            <button type="button" class="btn btn-sm" :disabled="auditBusy" @click="runAudit">
              <span v-if="auditBusy" class="spinner spinner-sm" aria-hidden="true" />
              <Icon v-else name="shield" :size="12" />
              {{ auditBusy ? t('skillHealth.auditing') : t('skillHealth.runAudit') }}
            </button>
          </div>
          <div class="card-body stack-sm">
            <ErrorBanner v-if="auditError" :message="auditError" :title="t('errors.auditFailed')" />
            <p v-if="!audit && !auditBusy" class="hint">{{ t('skillHealth.auditEmpty') }}</p>
            <template v-else-if="audit">
              <div class="row row-wrap" style="gap: 5px">
                <StatusBadge
                  :label="auditStatusLabel(audit.status)"
                  :tone="toneFor(auditStatusTone, audit.status)"
                  :dot="false"
                />
                <StatusBadge
                  :label="`${t('skillHealth.auditRisk')}：${riskLabel(audit.risk)}`"
                  :tone="toneFor(auditRiskTone, audit.risk)"
                  :dot="false"
                />
                <span class="chip">{{ audit.versionLabel }}</span>
                <span class="chip">{{ audit.kind }}</span>
                <span class="spacer" />
                <span class="hint">{{ formatDateTime(audit.createdAt, locale) }}</span>
              </div>
              <p class="text-sm">{{ audit.summary || '—' }}</p>
              <div v-if="audit.findings.length > 0" class="table-wrap list-scroll" style="max-height: 260px">
                <table class="table table-compact">
                  <thead>
                    <tr>
                      <th scope="col">{{ t('skillHealth.findingSeverity') }}</th>
                      <th scope="col">{{ t('skillHealth.findingCategory') }}</th>
                      <th scope="col">{{ t('skillHealth.findingTitle') }}</th>
                      <th scope="col">{{ t('skillHealth.findingDetail') }}</th>
                      <th scope="col">{{ t('common.path') }}</th>
                      <th scope="col">{{ t('skillHealth.findingSuggestion') }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="finding in audit.findings" :key="finding.id">
                      <td>
                        <StatusBadge
                          :label="findingSeverityLabel(finding.severity)"
                          :tone="toneFor(auditRiskTone, finding.severity)"
                          :dot="false"
                        />
                      </td>
                      <td><span class="chip">{{ findingCategoryLabel(finding.category) }}</span></td>
                      <td style="font-weight: 550">{{ finding.title }}</td>
                      <td class="hint">{{ finding.detail }}</td>
                      <td class="path-text">{{ finding.path || '—' }}</td>
                      <td class="hint">{{ finding.suggestion || '—' }}</td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <p v-else class="hint">{{ t('skillHealth.auditNoFindings') }}</p>
            </template>
          </div>
        </div>

        <!-- Dependencies & environment -->
        <div class="card">
          <div class="card-header">
            <div>
              <p class="card-title">{{ t('skillHealth.dependenciesTitle') }}</p>
              <p class="card-subtitle">{{ t('skillHealth.dependenciesLead') }}</p>
            </div>
            <button type="button" class="btn btn-sm" :disabled="depsBusy" @click="checkDependencies">
              <span v-if="depsBusy" class="spinner spinner-sm" aria-hidden="true" />
              <Icon v-else name="package" :size="12" />
              {{ depsBusy ? t('skillHealth.checkingDependencies') : t('skillHealth.checkDependencies') }}
            </button>
          </div>
          <div class="card-body stack-sm">
            <ErrorBanner
              v-if="depsError"
              :message="depsError"
              :title="t('errors.dependenciesFailed')"
            />
            <p v-if="!dependencies && !depsBusy" class="hint">
              {{ t('skillHealth.dependenciesEmpty') }}
            </p>
            <template v-else-if="dependencies">
              <div class="row row-wrap" style="gap: 5px">
                <StatusBadge
                  :label="environmentStatusLabel(dependencies.status)"
                  :tone="toneFor(environmentStatusTone, dependencies.status)"
                  :dot="false"
                />
                <StatusBadge
                  :label="`${t('skillHealth.auditRisk')}：${riskLabel(dependencies.risk)}`"
                  :tone="toneFor(auditRiskTone, dependencies.risk)"
                  :dot="false"
                />
                <span class="spacer" />
                <span class="hint">{{ formatDateTime(dependencies.checkedAt, locale) }}</span>
              </div>
              <p class="text-sm">{{ dependencies.summary || '—' }}</p>
              <div
                v-if="dependencies.items.length > 0"
                class="table-wrap list-scroll"
                style="max-height: 260px"
              >
                <table class="table table-compact">
                  <thead>
                    <tr>
                      <th scope="col">{{ t('common.name') }}</th>
                      <th scope="col">{{ t('skillHealth.dependencyKind') }}</th>
                      <th scope="col">{{ t('skillHealth.requiredVersion') }}</th>
                      <th scope="col">{{ t('skillHealth.foundVersion') }}</th>
                      <th scope="col">{{ t('common.status') }}</th>
                      <th scope="col">{{ t('skillHealth.findingDetail') }}</th>
                      <th scope="col">{{ t('skillHealth.findingSuggestion') }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="item in dependencies.items" :key="`${item.kind}-${item.name}`">
                      <td class="mono">{{ item.name }}</td>
                      <td><span class="chip">{{ dependencyKindLabel(item.kind) }}</span></td>
                      <td class="mono">{{ item.requiredVersion || '—' }}</td>
                      <td class="mono">{{ item.found || '—' }}</td>
                      <td>
                        <StatusBadge
                          :label="dependencyStatusLabel(item.status)"
                          :tone="toneFor(dependencyStatusTone, item.status)"
                          :dot="false"
                        />
                      </td>
                      <td class="hint">{{ item.detail || '—' }}</td>
                      <td class="hint">{{ item.suggestion || '—' }}</td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <p v-else class="hint">{{ t('skillHealth.dependenciesNoItems') }}</p>
            </template>
          </div>
        </div>
      </div>

      <div class="section-stack">
        <!-- Update impact -->
        <div class="card">
          <div class="card-header">
            <div>
              <p class="card-title">{{ t('skillHealth.impactTitle') }}</p>
              <p class="card-subtitle">{{ t('skillHealth.impactLead') }}</p>
            </div>
          </div>
          <div class="card-body stack-sm">
            <div class="field">
              <label class="label" for="impact-target-version">
                {{ t('skillHealth.impactTarget') }}
              </label>
              <select
                id="impact-target-version"
                v-model="impactTargetId"
                class="select"
                :disabled="targetVersions.length === 0"
              >
                <option v-for="version in targetVersions" :key="version.id" :value="version.id">
                  {{ version.versionLabel }}{{ version.isCurrent ? ` · ${t('skillDetail.current')}` : '' }}
                </option>
              </select>
            </div>
            <p class="hint">{{ t('skillHealth.impactSameVersionHint') }}</p>
            <div class="row row-wrap">
              <button
                type="button"
                class="btn btn-primary btn-sm"
                :disabled="!canAnalyzeImpact"
                @click="analyzeImpact"
              >
                <span v-if="impactBusy" class="spinner spinner-sm" aria-hidden="true" />
                <Icon v-else name="eye" :size="12" />
                {{ impactBusy ? t('skillHealth.analyzingImpact') : t('skillHealth.analyzeImpact') }}
              </button>
              <span v-if="impactTargetIsCurrent" class="hint">
                {{ t('skillHealth.impactSameVersion') }}
              </span>
            </div>
            <ErrorBanner v-if="impactError" :message="impactError" :title="t('errors.impactFailed')" />
            <UpdateImpactSummary v-if="impact" :impact="impact" show-files />
            <p v-else-if="!impactBusy" class="hint">{{ t('skillHealth.impactEmpty') }}</p>
          </div>
        </div>

        <!-- Validation evidence -->
        <div class="card">
          <div class="card-header">
            <div>
              <p class="card-title">{{ t('skillHealth.validationTitle') }}</p>
              <p class="card-subtitle">{{ t('skillHealth.validationLead') }}</p>
            </div>
            <button type="button" class="btn btn-sm btn-primary" @click="openRecord">
              <Icon name="plus" :size="12" />
              {{ t('skillHealth.recordValidation') }}
            </button>
          </div>
          <div class="card-body stack-sm">
            <div class="notice notice-warn">
              <Icon class="notice-icon" name="alert-triangle" :size="14" />
              <div class="notice-body">
                <p class="notice-title">{{ t('skillHealth.validationEvidenceTitle') }}</p>
                <p>{{ t('skillHealth.validationNotice') }}</p>
              </div>
            </div>
            <ErrorBanner
              v-if="validationsError"
              :message="validationsError"
              :title="t('errors.validationsFailed')"
            />
            <LoadingState v-if="validationsLoading && validations.length === 0" :label="t('health.loading')" compact />
            <p v-else-if="validations.length === 0" class="hint">
              {{ t('skillHealth.validationsEmpty') }}
            </p>
            <div v-else class="stack-sm list-scroll" style="max-height: 380px">
              <div
                v-for="run in validations"
                :key="run.id"
                class="surface-inset"
                style="padding: 9px"
              >
                <div class="row row-wrap" style="gap: 5px">
                  <StatusBadge
                    :label="validationStatusLabel(run.status)"
                    :tone="toneFor(validationStatusTone, run.status)"
                    :dot="false"
                  />
                  <span class="chip">{{ validationKindLabel(run.kind) }}</span>
                  <span class="text-sm" style="font-weight: 600">{{ run.caseName }}</span>
                  <span class="spacer" />
                  <span class="hint">{{ formatRelativeTime(run.createdAt, locale) }}</span>
                </div>
                <p class="hint">
                  {{ run.agentName || t('skillHealth.noAgentOption') }} · {{ run.versionLabel }} ·
                  {{ run.actor }} · {{ formatDateTime(run.createdAt, locale) }}
                </p>
                <p v-if="run.notes" class="text-sm">{{ run.notes }}</p>
                <pre v-if="run.evidence" class="code-block" style="margin: 6px 0 0; max-height: 140px">{{ run.evidence }}</pre>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <BaseModal
      v-if="recordOpen"
      :title="t('skillHealth.recordTitle')"
      :subtitle="t('skillHealth.recordLead')"
      size="md"
      @close="closeRecord"
    >
      <div class="stack-md">
        <div class="notice notice-warn">
          <Icon class="notice-icon" name="alert-triangle" :size="14" />
          <div class="notice-body">
            <p class="notice-title">{{ t('skillHealth.validationEvidenceTitle') }}</p>
            <p>{{ t('skillHealth.validationNotice') }}</p>
          </div>
        </div>

        <div class="field">
          <label class="label" for="validation-agent">{{ t('skillHealth.validationAgent') }}</label>
          <select id="validation-agent" v-model="recordForm.agentId" class="select">
            <option value="">{{ t('skillHealth.noAgentOption') }}</option>
            <option v-for="installation in installedAgents" :key="installation.agentId" :value="installation.agentId">
              {{ installation.agentName }} · {{ installation.versionLabel }}
            </option>
          </select>
          <p class="hint">{{ t('skillHealth.validationAgentHint') }}</p>
        </div>

        <div class="field">
          <label class="label" for="validation-case">{{ t('skillHealth.validationCase') }}</label>
          <input
            id="validation-case"
            v-model="recordForm.caseName"
            class="input"
            type="text"
            :placeholder="t('skillHealth.validationCasePlaceholder')"
          />
        </div>

        <div class="field">
          <label class="label" for="validation-status">{{ t('common.status') }}</label>
          <select id="validation-status" v-model="recordForm.status" class="select">
            <option v-for="status in VALIDATION_STATUSES" :key="status" :value="status">
              {{ validationStatusLabel(status) }}
            </option>
          </select>
        </div>

        <div class="field">
          <label class="label" for="validation-notes">{{ t('skillHealth.validationNotes') }}</label>
          <textarea
            id="validation-notes"
            v-model="recordForm.notes"
            class="textarea"
            rows="2"
            :placeholder="t('skillHealth.validationNotesPlaceholder')"
          />
        </div>

        <div class="field">
          <label class="label" for="validation-evidence">{{ t('skillHealth.validationEvidence') }}</label>
          <textarea
            id="validation-evidence"
            v-model="recordForm.evidence"
            class="textarea textarea-mono"
            rows="4"
            spellcheck="false"
            :placeholder="t('skillHealth.validationEvidencePlaceholder')"
          />
          <p class="hint">{{ t('skillHealth.validationEvidenceHint') }}</p>
        </div>

        <ErrorBanner
          v-if="recordError"
          :message="recordError"
          :title="t('errors.validationRecordFailed')"
        />
      </div>
      <template #footer>
        <button type="button" class="btn" :disabled="recordBusy" @click="closeRecord">
          {{ t('common.cancel') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="recordBusy" @click="submitRecord">
          <span v-if="recordBusy" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="check" :size="13" />
          {{ recordBusy ? t('common.saving') : t('skillHealth.recordSubmit') }}
        </button>
      </template>
    </BaseModal>
  </div>
</template>
