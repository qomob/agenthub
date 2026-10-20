<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink } from 'vue-router'

import { navItems } from '../router'
import { useAppStore } from '../stores/app'
import BaseModal from './BaseModal.vue'
import Icon from './Icon.vue'

const { t } = useI18n()
const app = useAppStore()
const showShortcuts = ref(false)

function openShortcuts(): void {
  showShortcuts.value = true
}

onMounted(() => window.addEventListener('agenthub:shortcuts', openShortcuts))
onBeforeUnmount(() => window.removeEventListener('agenthub:shortcuts', openShortcuts))

const groups = [
  { key: 'overview', labelKey: 'nav.groups.overview' },
  { key: 'capabilities', labelKey: 'nav.groups.capabilities' },
  { key: 'workspace', labelKey: 'nav.groups.workspace' },
  { key: 'validation', labelKey: 'nav.groups.validation' },
  { key: 'system', labelKey: 'nav.groups.system' },
] as const

const version = computed(() => app.settings?.appVersion || app.coreVersion || '')
const libraryRoot = computed(() => app.settings?.libraryRoot ?? '')

function itemsFor(group: string) {
  return navItems.filter((item) => item.group === group)
}
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar-brand">
      <div class="brand-mark" aria-hidden="true">AH</div>
      <div class="brand-text">
        <p class="brand-name">{{ t('common.appName') }}</p>
        <p class="brand-tagline">{{ t('common.tagline') }}</p>
      </div>
    </div>

    <nav class="sidebar-nav" aria-label="Main navigation">
      <template v-for="group in groups" :key="group.key">
        <p class="nav-group-label">{{ t(group.labelKey) }}</p>
        <RouterLink
          v-for="item in itemsFor(group.key)"
          :key="item.path"
          :to="item.path"
          class="nav-link"
          :title="t(item.labelKey)"
        >
          <Icon class="nav-icon" :name="item.icon" :size="16" />
          <span class="nav-label">{{ t(item.labelKey) }}</span>
          <span v-if="item.shortcut" class="nav-kbd">⌘{{ item.shortcut }}</span>
        </RouterLink>
      </template>
    </nav>

    <div class="sidebar-footer">
      <div class="sidebar-status" :title="libraryRoot">
        <span class="status-dot" aria-hidden="true" />
        <span class="truncate">{{ t('common.appName') }} {{ version }}</span>
      </div>
      <button type="button" class="sidebar-footer-btn" @click="openShortcuts">
        <span>{{ t('shortcuts.footer') }}</span>
        <span class="kbd">⌘/</span>
      </button>
    </div>

    <BaseModal
      v-if="showShortcuts"
      :title="t('shortcuts.title')"
      size="sm"
      @close="showShortcuts = false"
    >
      <div class="stack-sm">
        <div v-for="item in navItems" :key="item.path" class="info-row">
          <span class="info-row-label">{{ t(item.labelKey) }}</span>
          <span class="info-row-value">
            <span v-if="item.shortcut" class="kbd">⌘{{ item.shortcut }}</span>
            <span v-else class="subtle">—</span>
          </span>
        </div>
        <div class="info-row">
          <span class="info-row-label">{{ t('shortcuts.search') }}</span>
          <span class="info-row-value"><span class="kbd">⌘K</span></span>
        </div>
        <div class="info-row">
          <span class="info-row-label">{{ t('shortcuts.settings') }}</span>
          <span class="info-row-value"><span class="kbd">⌘,</span></span>
        </div>
        <div class="info-row">
          <span class="info-row-label">{{ t('shortcuts.close') }}</span>
          <span class="info-row-value"><span class="kbd">Esc</span></span>
        </div>
      </div>
      <p class="hint" style="margin-top: 10px">{{ t('shortcuts.navHint') }}</p>
    </BaseModal>
  </aside>
</template>
