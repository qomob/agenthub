<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import Icon from './Icon.vue'

withDefaults(
  defineProps<{
    message: string
    title?: string
    retryLabel?: string
    retrying?: boolean
  }>(),
  { title: undefined, retryLabel: undefined, retrying: false },
)

defineEmits<{ retry: [] }>()

const { t } = useI18n()
</script>

<template>
  <div class="error-banner" role="alert">
    <div class="notice-body">
      <p class="error-banner-title">{{ title ?? t('errors.title') }}</p>
      <p class="error-banner-message">{{ message }}</p>
    </div>
    <div class="row" style="flex: none">
      <button v-if="retryLabel" class="btn btn-sm" :disabled="retrying" @click="$emit('retry')">
        <span v-if="retrying" class="spinner spinner-sm" aria-hidden="true" />
        <Icon v-else name="refresh" :size="12" />
        {{ retryLabel }}
      </button>
      <slot name="actions" />
    </div>
  </div>
</template>
