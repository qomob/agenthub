<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { useUiStore, type ToastKind } from '../stores/ui'
import Icon from './Icon.vue'

const ui = useUiStore()
const { t } = useI18n()

const ICON: Record<ToastKind, string> = {
  success: 'check-circle',
  error: 'alert-circle',
  warning: 'alert-triangle',
  info: 'info',
}
</script>

<template>
  <Teleport to="body">
    <div class="toast-host" aria-live="polite" aria-atomic="false">
      <div
        v-for="toast in ui.toasts"
        :key="toast.id"
        class="toast"
        :class="`toast-${toast.kind}`"
        :role="toast.kind === 'error' ? 'alert' : 'status'"
      >
        <Icon class="toast-icon" :name="ICON[toast.kind]" :size="15" />
        <div class="toast-body">
          <p class="toast-message">{{ toast.message }}</p>
          <p v-if="toast.detail" class="toast-detail">{{ toast.detail }}</p>
        </div>
        <button
          type="button"
          class="toast-close"
          :aria-label="t('common.dismiss')"
          @click="ui.dismiss(toast.id)"
        >
          <Icon name="close" :size="12" />
        </button>
      </div>
    </div>
  </Teleport>
</template>
