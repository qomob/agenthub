<script setup lang="ts" generic="T extends object">
withDefaults(
  defineProps<{
    columns: Array<{
      key: string
      label: string
      align?: 'left' | 'right' | 'center'
      width?: string
      mono?: boolean
    }>
    rows: T[]
    rowKey?: (row: T, index: number) => string
    clickable?: boolean
    emptyText?: string
    compact?: boolean
  }>(),
  {
    rowKey: undefined,
    clickable: false,
    emptyText: '—',
    compact: false,
  },
)

const emit = defineEmits<{ rowClick: [row: T] }>()

function cellValue(row: T, key: string): unknown {
  return (row as Record<string, unknown>)[key]
}

function display(value: unknown): string {
  if (value === null || value === undefined || value === '') return '—'
  if (Array.isArray(value)) return value.length > 0 ? value.join(', ') : '—'
  if (typeof value === 'object') {
    try {
      return JSON.stringify(value)
    } catch {
      return '—'
    }
  }
  return String(value)
}

function onRowClick(row: T): void {
  emit('rowClick', row)
}
</script>

<template>
  <div class="table-wrap">
    <table class="table" :class="{ 'table-compact': compact }">
      <thead>
        <tr>
          <th
            v-for="column in columns"
            :key="column.key"
            :style="{ width: column.width, textAlign: column.align ?? 'left' }"
            scope="col"
          >
            {{ column.label }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-if="rows.length === 0">
          <td :colspan="columns.length" class="muted" style="text-align: center; padding: 22px">
            {{ emptyText }}
          </td>
        </tr>
        <tr
          v-for="(row, index) in rows"
          :key="rowKey ? rowKey(row, index) : index"
          :class="{ 'row-click': clickable }"
          :tabindex="clickable ? 0 : undefined"
          @click="clickable ? onRowClick(row) : undefined"
          @keydown.enter="clickable ? onRowClick(row) : undefined"
        >
          <td
            v-for="column in columns"
            :key="column.key"
            :class="{ num: column.mono }"
            :style="{ textAlign: column.align ?? 'left' }"
          >
            <slot :name="`cell-${column.key}`" :row="row" :value="cellValue(row, column.key)">
              <span :class="{ mono: column.mono }">{{ display(cellValue(row, column.key)) }}</span>
            </slot>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
