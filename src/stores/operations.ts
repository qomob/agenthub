import { defineStore } from 'pinia'
import { ref } from 'vue'

import { api, normalizeError } from '../lib/api'
import type {
  BackupResult,
  BackupView,
  ChangeCheck,
  DoctorReport,
  OperationResult,
  OperationView,
  SnapshotView,
} from '../lib/types'

export const useOperationsStore = defineStore('operations', () => {
  const operations = ref<OperationView[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const limit = ref(60)

  const snapshots = ref<SnapshotView[]>([])
  const snapshotsLoading = ref(false)
  const snapshotsError = ref<string | null>(null)

  const backups = ref<BackupView[]>([])
  const backupsLoading = ref(false)
  const backupsError = ref<string | null>(null)

  const doctor = ref<DoctorReport | null>(null)
  const doctorLoading = ref(false)
  const doctorError = ref<string | null>(null)

  const changeCheck = ref<ChangeCheck | null>(null)
  const changesLoading = ref(false)
  const changesError = ref<string | null>(null)

  async function loadOperations(nextLimit = limit.value): Promise<OperationView[]> {
    limit.value = nextLimit
    loading.value = true
    error.value = null
    try {
      operations.value = await api.operationsList(nextLimit)
      return operations.value
    } catch (err) {
      error.value = normalizeError(err, '无法加载活动记录')
      throw err
    } finally {
      loading.value = false
    }
  }

  async function loadOperation(id: string): Promise<OperationView> {
    return api.operationGet(id)
  }

  async function rollback(id: string): Promise<OperationResult> {
    const result = await api.rollbackOperation(id)
    await loadOperations().catch(() => undefined)
    return result
  }

  async function loadSnapshots(operationId?: string): Promise<SnapshotView[]> {
    snapshotsLoading.value = true
    snapshotsError.value = null
    try {
      snapshots.value = await api.snapshotsList(operationId, 200)
      return snapshots.value
    } catch (err) {
      snapshotsError.value = normalizeError(err, '无法加载快照')
      throw err
    } finally {
      snapshotsLoading.value = false
    }
  }

  async function loadBackups(): Promise<BackupView[]> {
    backupsLoading.value = true
    backupsError.value = null
    try {
      backups.value = await api.backupsList()
      return backups.value
    } catch (err) {
      backupsError.value = normalizeError(err, '无法加载备份')
      throw err
    } finally {
      backupsLoading.value = false
    }
  }

  async function createBackup(): Promise<BackupResult> {
    const result = await api.backupsCreate()
    await loadBackups().catch(() => undefined)
    return result
  }

  async function loadDoctor(): Promise<DoctorReport> {
    doctorLoading.value = true
    doctorError.value = null
    try {
      doctor.value = await api.doctor()
      return doctor.value
    } catch (err) {
      doctorError.value = normalizeError(err, '诊断失败')
      throw err
    } finally {
      doctorLoading.value = false
    }
  }

  async function loadChanges(): Promise<ChangeCheck> {
    changesLoading.value = true
    changesError.value = null
    try {
      changeCheck.value = await api.checkChanges()
      return changeCheck.value
    } catch (err) {
      changesError.value = normalizeError(err, '变更检测失败')
      throw err
    } finally {
      changesLoading.value = false
    }
  }

  return {
    operations,
    loading,
    error,
    limit,
    snapshots,
    snapshotsLoading,
    snapshotsError,
    backups,
    backupsLoading,
    backupsError,
    doctor,
    doctorLoading,
    doctorError,
    changeCheck,
    changesLoading,
    changesError,
    loadOperations,
    loadOperation,
    rollback,
    loadSnapshots,
    loadBackups,
    createBackup,
    loadDoctor,
    loadChanges,
  }
})
