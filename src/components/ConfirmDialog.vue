<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import BaseModal from './BaseModal.vue'

withDefaults(
  defineProps<{
    title: string
    message: string
    confirmLabel?: string
    danger?: boolean
    busy?: boolean
  }>(),
  { confirmLabel: undefined, danger: false, busy: false },
)

const emit = defineEmits<{ confirm: []; cancel: [] }>()
const { t } = useI18n()
</script>

<template>
  <BaseModal :title="title" size="sm" @close="emit('cancel')">
    <p class="text-sm" style="line-height: 1.7; white-space: pre-wrap">{{ message }}</p>
    <template #footer>
      <button type="button" class="btn" data-autofocus :disabled="busy" @click="emit('cancel')">
        {{ t('common.cancel') }}
      </button>
      <button
        type="button"
        class="btn"
        :class="danger ? 'btn-danger-solid' : 'btn-primary'"
        :disabled="busy"
        @click="emit('confirm')"
      >
        <span v-if="busy" class="spinner spinner-sm" aria-hidden="true" />
        {{ confirmLabel ?? t('common.confirm') }}
      </button>
    </template>
  </BaseModal>
</template>
