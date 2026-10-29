<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { normalizeError } from '../lib/api'
import { chooseDirectory } from '../lib/dialogs'
import { countDefaultImportableGroups, groupDiscoveredSkills } from '../lib/discovered'
import { formatDateTime } from '../lib/format'
import {
  agentStatusKey,
  scopeKey,
  toneForAgentStatus,
} from '../lib/status'
import type { DiscoveredSkill, ScanReport } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useUiStore } from '../stores/ui'

interface DiscoveredRow extends DiscoveredSkill {
  agentId: string
  agentName: string
}

const steps = ['privacy', 'library', 'agents', 'scan', 'finish'] as const

const { t, locale } = useI18n()
const app = useAppStore()
const ui = useUiStore()
const router = useRouter()

const step = ref(0)
const currentKey = computed(() => steps[step.value])
const scanning = computed(() => app.scanning)
const scanReport = ref<ScanReport | null>(app.scanReport)

const agreed = ref(false)
const libraryDraft = ref('')
const libraryBusy = ref(false)
const libraryError = ref<string | null>(null)
const libraryApplied = ref(false)

const refreshingAgents = ref(false)
const scanError = ref<string | null>(null)

const finishing = ref(false)
const finishError = ref<string | null>(null)

const libraryPath = computed(() => app.settings?.libraryRoot ?? '')
const agents = computed(() => app.agents)
const detectedAgents = computed(
  () =>
    app.agents.filter(
      (agent) => agent.status === 'installed' || agent.status === 'verification_required',
    ).length,
)

const discovered = computed<DiscoveredRow[]>(() => {
  if (!scanReport.value) return []
  return scanReport.value.agents.flatMap((agentScan) =>
    agentScan.skills.map((skill) => ({
      ...skill,
      agentId: agentScan.agentId,
      agentName: agentScan.agentName,
    })),
  )
})
const importableCount = computed(() =>
  countDefaultImportableGroups(groupDiscoveredSkills(scanReport.value)),
)
const invalidSkills = computed(() => discovered.value.filter((skill) => !skill.valid))
const conflictSkills = computed(() => discovered.value.filter((skill) => skill.conflict))
const agentScanErrors = computed(
  () =>
    scanReport.value?.agents.filter(
      (agent) => agent.errors.length > 0 || agent.roots.some((root) => root.errors.length > 0),
    ) ?? [],
)

const canAdvance = computed(() => {
  switch (steps[step.value]) {
    case 'privacy':
      return agreed.value
    case 'library':
      return !libraryBusy.value
    case 'scan':
      return scanReport.value !== null
    default:
      return true
  }
})

onMounted(() => {
  libraryDraft.value = app.settings?.libraryRoot ?? ''
})

async function selectLibrary(): Promise<void> {
  libraryError.value = null
  try {
    const selected = await chooseDirectory(libraryPath.value)
    if (selected) {
      libraryDraft.value = selected
      libraryApplied.value = false
    }
  } catch (error) {
    libraryError.value = normalizeError(error, t('errors.dialogFailed'))
  }
}

async function applyLibrary(): Promise<void> {
  const next = libraryDraft.value.trim()
  if (!next) return
  if (next === libraryPath.value) {
    libraryApplied.value = true
    return
  }
  libraryBusy.value = true
  libraryError.value = null
  try {
    await app.updateSettings({ libraryRoot: next })
    libraryApplied.value = true
    ui.notify('success', t('onboarding.library.applied'))
  } catch (error) {
    libraryError.value = normalizeError(error, t('errors.settingsFailed'))
  } finally {
    libraryBusy.value = false
  }
}

async function refreshAgents(): Promise<void> {
  refreshingAgents.value = true
  try {
    await app.refreshAgents()
  } catch (error) {
    ui.notifyError(t('errors.agentsFailed'), normalizeError(error))
  } finally {
    refreshingAgents.value = false
  }
}

async function runScan(): Promise<void> {
  scanError.value = null
  try {
    scanReport.value = await app.scan({ refreshDashboard: false })
    ui.notify('success', t('onboarding.scan.done'))
  } catch (error) {
    scanError.value = normalizeError(error, t('onboarding.scan.failed'))
  }
}

function next(): void {
  if (!canAdvance.value) return
  if (step.value < steps.length - 1) step.value += 1
}

function back(): void {
  if (step.value > 0) step.value -= 1
}

async function finish(): Promise<void> {
  if (!scanReport.value) {
    scanError.value = t('onboarding.scan.scanRequired')
    return
  }
  finishing.value = true
  finishError.value = null
  try {
    await app.markInitialized()
    ui.notify('success', t('onboarding.finish.done'))
    await router.push({ name: 'dashboard' })
  } catch (error) {
    finishError.value = normalizeError(error, t('errors.saveFailed'))
  } finally {
    finishing.value = false
  }
}
</script>

<template>
  <div class="onboarding">
    <aside class="onboarding-aside">
      <div class="sidebar-brand" style="padding: 0 0 8px">
        <div class="brand-mark" aria-hidden="true">AH</div>
        <div class="brand-text">
          <p class="brand-name">{{ t('common.appName') }}</p>
          <p class="brand-tagline">{{ t('common.tagline') }}</p>
        </div>
      </div>

      <div>
        <p class="onboarding-title" style="font-size: 17px">{{ t('onboarding.title') }}</p>
        <p class="onboarding-lead" style="margin-top: 6px">{{ t('onboarding.subtitle') }}</p>
      </div>

      <nav class="step-list" aria-label="Setup steps">
        <div
          v-for="(item, index) in steps"
          :key="item"
          class="step-item"
          :class="{ active: index === step, done: index < step }"
        >
          <span class="step-index">
            <Icon v-if="index < step" name="check" :size="11" />
            <template v-else>{{ index + 1 }}</template>
          </span>
          <span>{{ t(`onboarding.steps.${item}`) }}</span>
        </div>
      </nav>

      <div class="spacer" />
      <div class="notice notice-info">
        <Icon class="notice-icon" name="shield" :size="14" />
        <div class="notice-body">
          <p class="notice-title">{{ t('onboarding.privacy.title') }}</p>
          <p class="hint" style="color: var(--ah-fg)">{{ t('onboarding.privacy.points.local') }}</p>
        </div>
      </div>
    </aside>

    <main class="onboarding-main">
      <div class="onboarding-content">
        <p class="hint">
          {{ t('onboarding.stepOf', { current: step + 1, total: steps.length }) }}
        </p>
        <h1 class="onboarding-title">{{ t(`onboarding.${currentKey}.title`) }}</h1>
        <p class="onboarding-lead">{{ t(`onboarding.${currentKey}.lead`) }}</p>

        <!-- Step 1: privacy -->
        <section v-if="currentKey === 'privacy'" class="stack-md">
          <div class="card">
            <div class="card-body">
              <ul class="bullet-list">
                <li>{{ t('onboarding.privacy.points.local') }}</li>
                <li>{{ t('onboarding.privacy.points.read') }}</li>
                <li>{{ t('onboarding.privacy.points.write') }}</li>
                <li>{{ t('onboarding.privacy.points.git') }}</li>
                <li>{{ t('onboarding.privacy.points.keychain') }}</li>
              </ul>
            </div>
          </div>
          <label class="consent-box">
            <input v-model="agreed" type="checkbox" class="checkbox" />
            <span>
              <span class="text-sm" style="font-weight: 600; display: block">
                {{ t('onboarding.privacy.agree') }}
              </span>
              <span v-if="!agreed" class="hint">{{ t('onboarding.privacy.disagreeHint') }}</span>
            </span>
          </label>
        </section>

        <!-- Step 2: library -->
        <section v-else-if="currentKey === 'library'" class="stack-md">
          <div class="card">
            <div class="card-body stack-md">
              <div class="field">
                <span class="label">{{ t('onboarding.library.current') }}</span>
                <p class="path-text">{{ libraryPath || '—' }}</p>
              </div>

              <div class="field">
                <span class="label">{{ t('onboarding.library.chosen') }}</span>
                <input
                  v-model="libraryDraft"
                  class="input input-mono"
                  type="text"
                  spellcheck="false"
                  :placeholder="libraryPath"
                  :disabled="libraryBusy"
                />
                <p class="hint">{{ t('onboarding.library.note') }}</p>
              </div>

              <div class="row row-wrap">
                <button type="button" class="btn" :disabled="libraryBusy" @click="selectLibrary">
                  <Icon name="folder" :size="13" />
                  {{ t('onboarding.library.choose') }}
                </button>
                <button
                  type="button"
                  class="btn btn-primary"
                  :disabled="libraryBusy || !libraryDraft.trim() || libraryDraft === libraryPath"
                  @click="applyLibrary"
                >
                  <span v-if="libraryBusy" class="spinner spinner-sm" aria-hidden="true" />
                  <Icon v-else name="check" :size="13" />
                  {{ libraryBusy ? t('onboarding.library.applying') : t('onboarding.library.apply') }}
                </button>
                <span v-if="libraryApplied" class="badge badge-success">
                  {{ t('onboarding.library.applied') }}
                </span>
              </div>

              <ErrorBanner v-if="libraryError" :message="libraryError" :title="t('errors.settingsFailed')" />
              <p class="hint">{{ t('onboarding.library.emptyDirHint') }}</p>
            </div>
          </div>
        </section>

        <!-- Step 3: agents -->
        <section v-else-if="currentKey === 'agents'" class="stack-md">
          <div class="row row-wrap" style="justify-content: space-between">
            <div class="row">
              <span class="badge badge-success">{{ t('onboarding.agents.detected') }} {{ detectedAgents }}</span>
              <span class="badge badge-neutral">{{ t('onboarding.agents.total') }} {{ agents.length }}</span>
            </div>
            <button type="button" class="btn" :disabled="refreshingAgents" @click="refreshAgents">
              <span v-if="refreshingAgents" class="spinner spinner-sm" aria-hidden="true" />
              <Icon v-else name="refresh" :size="13" />
              {{ refreshingAgents ? t('onboarding.agents.refreshing') : t('onboarding.agents.refresh') }}
            </button>
          </div>

          <div v-if="agents.length === 0" class="card">
            <div class="card-body">
              <p class="text-sm">{{ t('onboarding.agents.noAgents') }}</p>
            </div>
          </div>

          <div v-else class="grid-cards">
            <article v-for="agent in agents" :key="agent.id" class="card">
              <div class="card-body stack-sm">
                <div class="agent-card-header">
                  <div class="row" style="min-width: 0">
                    <div class="agent-avatar" aria-hidden="true">{{ agent.name.slice(0, 2) }}</div>
                    <div style="min-width: 0">
                      <p style="font-weight: 600" :title="agent.name">{{ agent.name }}</p>
                      <p class="hint">{{ agent.vendor }}</p>
                    </div>
                  </div>
                  <StatusBadge
                    :label="t(agentStatusKey[agent.status])"
                    :tone="toneForAgentStatus(agent.status)"
                  />
                </div>
                <div class="stack-sm">
                  <p v-if="agent.appPath" class="path-text" :title="agent.appPath">
                    {{ agent.appPath }}
                  </p>
                  <p v-if="agent.homePath" class="path-text" :title="agent.homePath">
                    {{ agent.homePath }}
                  </p>
                  <p class="hint">{{ t('onboarding.agents.roots', { count: agent.skillRoots.length }) }}</p>
                </div>
                <div class="row row-wrap" style="gap: 4px">
                  <span
                    v-for="root in agent.skillRoots"
                    :key="root.path"
                    class="chip"
                    :title="root.path"
                  >
                    {{ t(scopeKey[root.scope]) }} · {{ root.path }}
                  </span>
                </div>
              </div>
            </article>
          </div>
        </section>

        <!-- Step 4: scan -->
        <section v-else-if="currentKey === 'scan'" class="stack-md">
          <div class="row row-wrap">
            <button type="button" class="btn btn-primary" :disabled="scanning" @click="runScan">
              <span v-if="scanning" class="spinner spinner-sm" aria-hidden="true" />
              <Icon v-else name="scan" :size="13" />
              {{
                scanning
                  ? t('onboarding.scan.running')
                  : scanReport
                    ? t('onboarding.scan.again')
                    : t('onboarding.scan.start')
              }}
            </button>
            <span v-if="scanReport" class="text-sm muted">
              {{ t('onboarding.scan.at') }}：{{ formatDateTime(scanReport.scannedAt, locale) }}
            </span>
          </div>

          <ErrorBanner v-if="scanError" :message="scanError" :title="t('onboarding.scan.failed')" />

          <template v-if="scanReport">
            <div class="stat-grid">
              <div class="stat-card">
                <div class="stat-head">{{ t('onboarding.scan.total') }}</div>
                <div class="stat-value">{{ scanReport.totalSkills }}</div>
              </div>
              <div class="stat-card">
                <div class="stat-head">{{ t('onboarding.scan.duplicates') }}</div>
                <div class="stat-value">{{ scanReport.duplicates }}</div>
              </div>
              <div class="stat-card" :class="{ 'stat-tone-warn': scanReport.conflicts > 0 }">
                <div class="stat-head">{{ t('onboarding.scan.conflicts') }}</div>
                <div class="stat-value">{{ scanReport.conflicts }}</div>
              </div>
              <div class="stat-card" :class="{ 'stat-tone-danger': scanReport.invalid > 0 }">
                <div class="stat-head">{{ t('onboarding.scan.invalid') }}</div>
                <div class="stat-value">{{ scanReport.invalid }}</div>
              </div>
            </div>

            <div v-if="importableCount > 0" class="notice notice-info">
              <Icon class="notice-icon" name="import" :size="14" />
              <div class="notice-body">
                <p class="notice-title">
                  {{ t('onboarding.scan.importableTitle', { count: importableCount }) }}
                </p>
                <p>{{ t('onboarding.scan.importableHint') }}</p>
              </div>
            </div>

            <div v-if="scanReport.warnings.length > 0" class="notice notice-warn">
              <Icon class="notice-icon" name="alert-triangle" :size="14" />
              <div class="notice-body">
                <p class="notice-title">{{ t('onboarding.scan.warnings') }}</p>
                <ul class="list-reset stack-sm">
                  <li v-for="(warning, index) in scanReport.warnings" :key="index">{{ warning }}</li>
                </ul>
              </div>
            </div>

            <div class="card">
              <div class="card-header">
                <div>
                  <p class="card-title">{{ t('onboarding.scan.discoveredTitle') }}</p>
                  <p class="card-subtitle">{{ discovered.length }} {{ t('common.items') }}</p>
                </div>
              </div>
              <div class="card-body flush">
                <div v-if="discovered.length === 0" class="card-body">
                  <p class="text-sm muted">{{ t('onboarding.scan.noDiscoveries') }}</p>
                </div>
                <div v-else class="table-wrap list-scroll">
                  <table class="table table-compact">
                    <thead>
                      <tr>
                        <th scope="col">{{ t('common.name') }}</th>
                        <th scope="col">{{ t('nav.agents') }}</th>
                        <th scope="col">{{ t('common.status') }}</th>
                        <th scope="col">{{ t('common.path') }}</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-for="skill in discovered" :key="`${skill.agentId}:${skill.path}`">
                        <td>
                          <div class="row" style="gap: 5px">
                            <span style="font-weight: 550">{{ skill.name }}</span>
                            <span v-if="skill.version" class="chip">{{ skill.version }}</span>
                          </div>
                          <p class="hint">{{ skill.slug }}</p>
                        </td>
                        <td>
                          <div>{{ skill.agentName }}</div>
                          <p class="hint">{{ t(scopeKey[skill.scope]) }}</p>
                        </td>
                        <td>
                          <div class="row row-wrap" style="gap: 4px">
                            <StatusBadge
                              v-if="!skill.valid"
                              :label="t('onboarding.scan.invalidBadge')"
                              tone="danger"
                            />
                            <StatusBadge
                              v-if="skill.conflict"
                              :label="t('onboarding.scan.conflictBadge')"
                              tone="warn"
                            />
                            <StatusBadge
                              v-if="skill.inSync"
                              :label="t('onboarding.scan.inSync')"
                              tone="success"
                            />
                            <StatusBadge
                              v-else-if="skill.valid && !skill.conflict"
                              :label="t('onboarding.scan.notInLibrary')"
                              tone="neutral"
                            />
                          </div>
                        </td>
                        <td class="path-text">{{ skill.path }}</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>
            </div>

            <div
              v-if="invalidSkills.length > 0 || conflictSkills.length > 0"
              class="grid-2"
              style="grid-template-columns: 1fr 1fr"
            >
              <div v-if="invalidSkills.length > 0" class="card">
                <div class="card-header">
                  <p class="card-title">{{ t('onboarding.scan.invalidTitle') }}</p>
                  <span class="badge badge-danger">{{ invalidSkills.length }}</span>
                </div>
                <div class="card-body stack-sm list-scroll">
                  <div v-for="skill in invalidSkills" :key="skill.path" class="stack-sm">
                    <p class="text-sm" style="font-weight: 550">{{ skill.name }} · {{ skill.agentName }}</p>
                    <p class="path-text">{{ skill.path }}</p>
                    <p v-for="(issue, index) in skill.issues" :key="index" class="hint tone-danger">
                      {{ issue }}
                    </p>
                  </div>
                </div>
              </div>
              <div v-if="conflictSkills.length > 0" class="card">
                <div class="card-header">
                  <p class="card-title">{{ t('onboarding.scan.conflictTitle') }}</p>
                  <span class="badge badge-warn">{{ conflictSkills.length }}</span>
                </div>
                <div class="card-body stack-sm list-scroll">
                  <div v-for="skill in conflictSkills" :key="skill.path" class="stack-sm">
                    <p class="text-sm" style="font-weight: 550">{{ skill.name }} · {{ skill.agentName }}</p>
                    <p class="path-text">{{ skill.path }}</p>
                    <p class="hint tone-warn">{{ skill.conflict }}</p>
                  </div>
                </div>
              </div>
            </div>

            <div v-if="agentScanErrors.length > 0" class="card">
              <div class="card-header">
                <p class="card-title">{{ t('onboarding.scan.agentErrors') }}</p>
              </div>
              <div class="card-body stack-sm">
                <div v-for="agent in agentScanErrors" :key="agent.agentId">
                  <p class="text-sm" style="font-weight: 550">{{ agent.agentName }}</p>
                  <p v-for="(error, index) in agent.errors" :key="`e-${index}`" class="hint tone-danger">
                    {{ error }}
                  </p>
                  <template v-for="root in agent.roots" :key="root.path">
                    <p v-for="(error, index) in root.errors" :key="`r-${index}`" class="hint tone-danger">
                      {{ root.path }}：{{ error }}
                    </p>
                  </template>
                </div>
              </div>
            </div>
          </template>

          <div v-else-if="!scanning" class="notice notice-info">
            <Icon class="notice-icon" name="info" :size="14" />
            <p>{{ t('onboarding.scan.scanRequired') }}</p>
          </div>
        </section>

        <!-- Step 5: finish -->
        <section v-else class="stack-md">
          <div class="card">
            <div class="card-body">
              <dl class="definition-grid">
                <dt>{{ t('onboarding.finish.libraryRoot') }}</dt>
                <dd class="path-text">{{ libraryPath || '—' }}</dd>
                <dt>{{ t('onboarding.finish.agentsDetected') }}</dt>
                <dd>{{ detectedAgents }} / {{ agents.length }}</dd>
                <dt>{{ t('onboarding.finish.skillsFound') }}</dt>
                <dd>{{ scanReport?.totalSkills ?? 0 }}</dd>
              </dl>
            </div>
          </div>
          <div class="notice notice-info">
            <Icon class="notice-icon" name="shield" :size="14" />
            <p>{{ t('onboarding.finish.note') }}</p>
          </div>
          <ErrorBanner v-if="finishError" :message="finishError" :title="t('errors.saveFailed')" />
        </section>

        <div class="onboarding-footer">
          <button type="button" class="btn" :disabled="step === 0" @click="back">
            <Icon name="arrow-left" :size="13" />
            {{ t('common.back') }}
          </button>
          <div class="row">
            <button
              v-if="step < steps.length - 1"
              type="button"
              class="btn btn-primary"
              :disabled="!canAdvance"
              @click="next"
            >
              {{ t('common.next') }}
              <Icon name="arrow-right" :size="13" />
            </button>
            <button
              v-else
              type="button"
              class="btn btn-primary"
              :disabled="finishing || !scanReport"
              @click="finish"
            >
              <span v-if="finishing" class="spinner spinner-sm" aria-hidden="true" />
              <Icon v-else name="check" :size="13" />
              {{ finishing ? t('onboarding.finish.finishing') : t('onboarding.finish.action') }}
            </button>
          </div>
        </div>
      </div>
    </main>
  </div>
</template>
