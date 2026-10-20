<script setup lang="ts">
import Icon from './Icon.vue'

withDefaults(
  defineProps<{
    title: string
    body?: string
    icon?: string
  }>(),
  { body: undefined, icon: 'inbox' },
)
</script>

<template>
  <div class="empty-state">
    <div class="empty-icon">
      <Icon :name="icon" :size="18" />
    </div>
    <p class="empty-title">{{ title }}</p>
    <p v-if="body" class="empty-body">{{ body }}</p>
    <div v-if="$slots.actions" class="empty-actions">
      <slot name="actions" />
    </div>
  </div>
</template>
