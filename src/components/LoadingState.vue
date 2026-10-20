<script setup lang="ts">
withDefaults(defineProps<{ label?: string; compact?: boolean }>(), {
  label: undefined,
  compact: false,
})
</script>

<template>
  <div class="loading-block" :style="compact ? { padding: '12px 0' } : undefined" role="status">
    <span class="spinner" aria-hidden="true" />
    <span>{{ label ?? '' }}</span>
    <slot />
  </div>
</template>
