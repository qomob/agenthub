<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { api, normalizeError } from '../lib/api'
import { chooseDirectory, chooseFile } from '../lib/dialogs'
import { splitLines } from '../lib/format'
import type { DiscoveredSkill, ImportRequest, ImportResult } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useSkillsStore } from '../stores/skills'
import { useUiStore } from '../stores/ui'
import BaseModal from './BaseModal.vue'
import ErrorBanner from './ErrorBanner.vue'
import Icon from './Icon.vue'
import StatusBadge from './StatusBadge.vue'

type SourceType = 'folder' | 'archive' | 'skill_md' | 'git' | 'scan'

interface ScanRow extends DiscoveredSkill {
  agentId: string
  agentName: string
}

const emit = defineEmits<{ close: []; imported: [skillId: string] }>()

const { t } = useI18n()
const app = useAppStore()
const skillsStore = useSkillsStore()
const ui = useUiStore()

const sources: Array<{ key: SourceType; labelKey: string; icon: string }> = [
  { key: 'folder', labelKey: 'skills.importDialog.sourceFolder', icon: 'folder' },
  { key: 'archive', labelKey: 'skills.importDialog.sourceArchive', icon: 'file' },
  { key: 'skill_md', labelKey: 'skills.importDialog.sourceSkillMd', icon: 'skills' },
  { key: 'git', labelKey: 'skills.importDialog.sourceGit', icon: 'git' },
  { key: 'scan', labelKey: 'skills.importDialog.sourceScan', icon: 'scan' },
]

const sourceType = ref<SourceType>('folder')
const folderPath = ref('')
const archivePath = ref('')
const skillMdPath = ref('')
const gitUrl = ref('')
const gitRef = ref('')
const selectedScanKey = ref<string | null>(null)
const skillPath = ref('')
const versionLabel = ref('')
const notes = ref('')

const busy = ref(false)
const error = ref<string | null>(null)
const result = ref<ImportResult | null>(null)
const scanning = computed(() => app.scanning)
const scanError = ref<string | null>(null)

const scanRows = computed<ScanRow[]>(() => {
  const report = app.scanReport
  if (!report) return []
  return report.agents.flatMap((agent) =>
    agent.skills.map((skill) => ({
      ...skill,
      agentId: agent.agentId,
      agentName: agent.agentName,
    })),
  )
})

const selectedScan = computed(
  () => scanRows.value.find((row) => scanKey(row) === selectedScanKey.value) ?? null,
)

function scanKey(row: ScanRow): string {
  return `${row.agentId}:${row.path}`
}

const canSubmit = computed(() => {
  switch (sourceType.value) {
    case 'folder':
      return folderPath.value.trim().length > 0
    case 'archive':
      return archivePath.value.trim().length > 0
    case 'skill_md':
      return skillMdPath.value.trim().length > 0
    case 'git':
      return gitUrl.value.trim().length > 0
    case 'scan':
      return selectedScan.value !== null
    default:
      return false
  }
})

async function pickFolder(): Promise<void> {
  try {
    const selected = await chooseDirectory()
    if (selected) folderPath.value = selected
  } catch (err) {
    ui.notifyError(t('errors.dialogFailed'), normalizeError(err))
  }
}

async function pickArchive(): Promise<void> {
  try {
    const selected = await chooseFile(['zip', 'tar', 'gz', 'tgz'])
    if (selected) archivePath.value = selected
  } catch (err) {
    ui.notifyError(t('errors.dialogFailed'), normalizeError(err))
  }
}

async function pickSkillMd(): Promise<void> {
  try {
    const selected = await chooseFile(['md', 'markdown'])
    if (selected) skillMdPath.value = selected
  } catch (err) {
    ui.notifyError(t('errors.dialogFailed'), normalizeError(err))
  }
}

async function runScan(): Promise<void> {
  scanError.value = null
  try {
    await app.scan({ refreshDashboard: false })
  } catch (err) {
    scanError.value = normalizeError(err, t('onboarding.scan.failed'))
  }
}

function buildRequest(): ImportRequest | null {
  let source: ImportRequest['source'] | null = null
  switch (sourceType.value) {
    case 'folder':
      source = { type: 'folder', path: folderPath.value.trim() }
      break
    case 'archive':
      source = { type: 'archive', path: archivePath.value.trim() }
      break
    case 'skill_md':
      source = { type: 'skill_md', path: skillMdPath.value.trim() }
      break
    case 'git':
      source = {
        type: 'git',
        url: gitUrl.value.trim(),
        reference: gitRef.value.trim() || null,
      }
      break
    case 'scan': {
      const row = selectedScan.value
      if (!row) return null
      source = { type: 'agent_dir', agent_id: row.agentId, path: row.path }
      break
    }
  }
  if (!source) return null
  return {
    source,
    skillPath: skillPath.value.trim() || null,
    versionLabel: versionLabel.value.trim() || null,
    notes: splitLines(notes.value).join('\n') || null,
  }
}

async function submit(): Promise<void> {
  const request = buildRequest()
  if (!request) {
    error.value = t('errors.invalidInput')
    return
  }
  busy.value = true
  error.value = null
  result.value = null
  try {
    const imported = await api.importSkill(request)
    result.value = imported
    await skillsStore.refresh()
    ui.notify('success', t('skills.importDialog.resultTitle'), {
      detail: imported.skill.name,
    })
  } catch (err) {
    error.value = normalizeError(err, t('errors.importFailed'))
  } finally {
    busy.value = false
  }
}

function openImported(): void {
  if (result.value) emit('imported', result.value.skill.id)
}
</script>

<template>
  <BaseModal :title="t('skills.import')" size="lg" @close="emit('close')">
    <div class="stack-md">
      <div class="tabs" role="tablist">
        <button
          v-for="source in sources"
          :key="source.key"
          type="button"
          role="tab"
          class="tab"
          :class="{ active: sourceType === source.key }"
          :aria-selected="sourceType === source.key"
          @click="sourceType = source.key"
        >
          {{ t(source.labelKey) }}
        </button>
      </div>

      <template v-if="sourceType === 'folder'">
        <div class="field">
          <span class="label">{{ t('skills.importDialog.folderLabel') }}</span>
          <div class="row">
            <input
              v-model="folderPath"
              class="input input-mono"
              type="text"
              spellcheck="false"
              :placeholder="t('skills.importDialog.folderLabel')"
            />
            <button type="button" class="btn" @click="pickFolder">
              <Icon name="folder" :size="13" />
              {{ t('common.chooseFolder') }}
            </button>
          </div>
          <p class="hint">{{ t('skills.importDialog.folderHint') }}</p>
        </div>
      </template>

      <template v-else-if="sourceType === 'archive'">
        <div class="field">
          <span class="label">{{ t('skills.importDialog.archiveLabel') }}</span>
          <div class="row">
            <input
              v-model="archivePath"
              class="input input-mono"
              type="text"
              spellcheck="false"
              :placeholder="t('skills.importDialog.archiveLabel')"
            />
            <button type="button" class="btn" @click="pickArchive">
              <Icon name="file" :size="13" />
              {{ t('common.chooseFile') }}
            </button>
          </div>
          <p class="hint">{{ t('skills.importDialog.archiveHint') }}</p>
        </div>
      </template>

      <template v-else-if="sourceType === 'skill_md'">
        <div class="field">
          <span class="label">{{ t('skills.importDialog.skillMdLabel') }}</span>
          <div class="row">
            <input
              v-model="skillMdPath"
              class="input input-mono"
              type="text"
              spellcheck="false"
              :placeholder="t('skills.importDialog.skillMdLabel')"
            />
            <button type="button" class="btn" @click="pickSkillMd">
              <Icon name="file" :size="13" />
              {{ t('common.chooseFile') }}
            </button>
          </div>
          <p class="hint">{{ t('skills.importDialog.skillMdHint') }}</p>
        </div>
      </template>

      <template v-else-if="sourceType === 'git'">
        <div class="field">
          <label class="label" for="import-git-url">{{ t('skills.importDialog.gitUrl') }}</label>
          <input
            id="import-git-url"
            v-model="gitUrl"
            class="input input-mono"
            type="text"
            spellcheck="false"
            placeholder="https://github.com/owner/repo.git"
          />
        </div>
        <div class="field">
          <label class="label" for="import-git-ref">{{ t('skills.importDialog.gitRef') }}</label>
          <input
            id="import-git-ref"
            v-model="gitRef"
            class="input input-mono"
            type="text"
            spellcheck="false"
            :placeholder="t('skills.importDialog.gitRefHint')"
          />
        </div>
        <div class="notice notice-warn">
          <Icon class="notice-icon" name="alert-triangle" :size="14" />
          <p>{{ t('skills.importDialog.gitWarning') }}</p>
        </div>
        <div v-if="app.settings && !app.settings.gitImportEnabled" class="notice notice-danger">
          <Icon class="notice-icon" name="lock" :size="14" />
          <p>{{ t('settings.gitImportDisabled') }}</p>
        </div>
      </template>

      <template v-else>
        <p class="hint">{{ t('skills.importDialog.scanHint') }}</p>
        <div class="row row-wrap">
          <button type="button" class="btn" :disabled="scanning" @click="runScan">
            <span v-if="scanning" class="spinner spinner-sm" aria-hidden="true" />
            <Icon v-else name="scan" :size="13" />
            {{ scanning ? t('onboarding.scan.running') : t('skills.importDialog.runScan') }}
          </button>
          <span v-if="app.scanReport" class="text-sm muted">
            {{ t('onboarding.scan.at') }}：{{ app.scanReport.scannedAt }}
          </span>
        </div>
        <ErrorBanner v-if="scanError" :message="scanError" :title="t('errors.scanFailed')" />
        <p v-if="!app.scanReport" class="hint">{{ t('skills.importDialog.scanEmpty') }}</p>
        <p v-else-if="scanRows.length === 0" class="hint">
          {{ t('skills.importDialog.scanEmpty') }}
        </p>
        <div v-else class="table-wrap surface-inset list-scroll" style="max-height: 260px">
          <table class="table table-compact">
            <thead>
              <tr>
                <th scope="col" style="width: 32px"></th>
                <th scope="col">{{ t('common.name') }}</th>
                <th scope="col">{{ t('onboarding.agents.title') }}</th>
                <th scope="col">{{ t('common.status') }}</th>
                <th scope="col">{{ t('common.path') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="row in scanRows"
                :key="scanKey(row)"
                class="row-click"
                @click="selectedScanKey = scanKey(row)"
              >
                <td>
                  <input
                    type="radio"
                    name="scan-skill"
                    class="checkbox"
                    :checked="selectedScanKey === scanKey(row)"
                    :aria-label="row.name"
                    @change="selectedScanKey = scanKey(row)"
                  />
                </td>
                <td>
                  <span style="font-weight: 550">{{ row.name }}</span>
                  <p class="hint">{{ row.slug }}</p>
                </td>
                <td>
                  {{ row.agentName }}
                  <p class="hint">{{ t(`status.scope.${row.scope}`) }}</p>
                </td>
                <td>
                  <div class="row row-wrap" style="gap: 4px">
                    <StatusBadge
                      v-if="!row.valid"
                      :label="t('onboarding.scan.invalidBadge')"
                      tone="danger"
                    />
                    <StatusBadge
                      v-if="row.conflict"
                      :label="t('onboarding.scan.conflictBadge')"
                      tone="warn"
                    />
                    <StatusBadge
                      v-if="row.inSync"
                      :label="t('onboarding.scan.inSync')"
                      tone="success"
                    />
                  </div>
                  <p v-for="(issue, index) in row.issues" :key="index" class="hint tone-warn">
                    {{ issue }}
                  </p>
                </td>
                <td class="path-text">{{ row.path }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </template>

      <details class="surface-inset" style="padding: 9px 11px">
        <summary class="text-sm" style="cursor: pointer; font-weight: 600">
          {{ t('skills.importDialog.advanced') }}
        </summary>
        <div class="stack-md" style="margin-top: 10px">
          <div v-if="sourceType === 'archive' || sourceType === 'git'" class="field">
            <label class="label" for="import-skill-path">{{ t('skills.importDialog.skillPath') }}</label>
            <input
              id="import-skill-path"
              v-model="skillPath"
              class="input input-mono"
              type="text"
              spellcheck="false"
              :placeholder="t('skills.importDialog.skillPathHint')"
            />
          </div>
          <div class="field">
            <label class="label" for="import-version">{{ t('skills.importDialog.versionLabel') }}</label>
            <input
              id="import-version"
              v-model="versionLabel"
              class="input"
              type="text"
              placeholder="1.0.0"
            />
          </div>
          <div class="field">
            <label class="label" for="import-notes">{{ t('skills.importDialog.notes') }}</label>
            <textarea
              id="import-notes"
              v-model="notes"
              class="textarea"
              rows="2"
              :placeholder="t('skills.importDialog.notesHint')"
            />
          </div>
        </div>
      </details>

      <ErrorBanner v-if="error" :message="error" :title="t('errors.importFailed')" />

      <div v-if="result" class="notice notice-success">
        <Icon class="notice-icon" name="check-circle" :size="14" />
        <div class="notice-body stack-sm">
          <p class="notice-title">
            {{ result.skill.name }} · {{ result.version.versionLabel }}
            <span class="badge" :class="result.created ? 'badge-success' : 'badge-info'">
              {{ result.created ? t('skills.importDialog.resultCreated') : t('skills.importDialog.resultUpdated') }}
            </span>
          </p>
          <ul v-if="result.warnings.length > 0" class="list-reset stack-sm">
            <li v-for="(warning, index) in result.warnings" :key="index" class="hint">
              ⚠ {{ warning }}
            </li>
          </ul>
        </div>
      </div>
    </div>

    <template #footer>
      <button type="button" class="btn" :disabled="busy" @click="emit('close')">
        {{ result ? t('common.close') : t('common.cancel') }}
      </button>
      <button v-if="result" type="button" class="btn btn-primary" @click="openImported">
        {{ t('skills.importDialog.resultOpen') }}
        <Icon name="arrow-right" :size="13" />
      </button>
      <button
        v-else
        type="button"
        class="btn btn-primary"
        :disabled="busy || !canSubmit"
        @click="submit"
      >
        <span v-if="busy" class="spinner spinner-sm" aria-hidden="true" />
        <Icon v-else name="import" :size="13" />
        {{ busy ? t('skills.importDialog.importing') : t('skills.import') }}
      </button>
    </template>
  </BaseModal>
</template>
