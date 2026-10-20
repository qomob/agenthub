<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, useId } from 'vue'
import { useI18n } from 'vue-i18n'

import { isTopModal, popModal, pushModal } from '../lib/modalStack'
import Icon from './Icon.vue'

const props = withDefaults(
  defineProps<{
    title: string
    subtitle?: string
    size?: 'sm' | 'md' | 'lg' | 'xl'
    drawer?: boolean
    closeOnBackdrop?: boolean
    closeOnEsc?: boolean
  }>(),
  {
    subtitle: undefined,
    size: 'md',
    drawer: false,
    closeOnBackdrop: true,
    closeOnEsc: true,
  },
)

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()

const panel = ref<HTMLElement | null>(null)
const titleId = useId()
let handle = 0
let lastActive: HTMLElement | null = null

function focusables(): HTMLElement[] {
  const root = panel.value
  if (!root) return []
  const selector =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
  return Array.from(root.querySelectorAll<HTMLElement>(selector)).filter(
    (element) => element.offsetParent !== null || element === document.activeElement,
  )
}

function onKeydown(event: KeyboardEvent): void {
  if (!isTopModal(handle)) return
  if (event.key === 'Escape' && props.closeOnEsc) {
    event.preventDefault()
    event.stopPropagation()
    emit('close')
    return
  }
  if (event.key !== 'Tab') return
  const items = focusables()
  if (items.length === 0) {
    event.preventDefault()
    panel.value?.focus()
    return
  }
  const first = items[0]
  const last = items[items.length - 1]
  const active = document.activeElement as HTMLElement | null
  if (event.shiftKey && (active === first || !panel.value?.contains(active))) {
    event.preventDefault()
    last.focus()
  } else if (!event.shiftKey && active === last) {
    event.preventDefault()
    first.focus()
  }
}

function onBackdrop(): void {
  if (props.closeOnBackdrop) emit('close')
}

onMounted(async () => {
  lastActive = document.activeElement instanceof HTMLElement ? document.activeElement : null
  handle = pushModal()
  document.addEventListener('keydown', onKeydown, true)
  await nextTick()
  const preferred =
    panel.value?.querySelector<HTMLElement>('[data-autofocus]') ?? focusables()[0] ?? panel.value
  preferred?.focus()
})

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown, true)
  popModal(handle)
  if (lastActive && document.contains(lastActive)) lastActive.focus()
})
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop" :class="{ 'is-drawer': drawer }" @click.self="onBackdrop">
      <div
        ref="panel"
        class="modal"
        :class="[`modal-${size}`, { drawer }]"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="titleId"
        tabindex="-1"
      >
        <header class="modal-header">
          <div style="min-width: 0">
            <h2 :id="titleId" class="modal-title">{{ title }}</h2>
            <p v-if="subtitle" class="modal-subtitle">{{ subtitle }}</p>
          </div>
          <button
            type="button"
            class="btn btn-ghost btn-icon btn-sm"
            :aria-label="t('common.closeDialog')"
            @click="emit('close')"
          >
            <Icon name="close" :size="14" />
          </button>
        </header>
        <div class="modal-body">
          <slot />
        </div>
        <footer v-if="$slots.footer" class="modal-footer">
          <slot name="footer" />
        </footer>
      </div>
    </div>
  </Teleport>
</template>
