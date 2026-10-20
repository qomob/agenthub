import { defineStore } from 'pinia'
import { ref } from 'vue'

import { api, normalizeError } from '../lib/api'
import type { Skill, SkillDetail } from '../lib/types'

export const useSkillsStore = defineStore('skills', () => {
  const skills = ref<Skill[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const loadedAt = ref<number>(0)

  async function load(force = false): Promise<Skill[]> {
    if (skills.value.length > 0 && !force && Date.now() - loadedAt.value < 15_000) {
      return skills.value
    }
    loading.value = true
    error.value = null
    try {
      skills.value = await api.skillsList()
      loadedAt.value = Date.now()
      return skills.value
    } catch (err) {
      error.value = normalizeError(err, '无法加载技能库')
      throw err
    } finally {
      loading.value = false
    }
  }

  async function refresh(): Promise<Skill[]> {
    return load(true)
  }

  async function getDetail(id: string): Promise<SkillDetail> {
    return api.skillGet(id)
  }

  function findById(id: string): Skill | null {
    return skills.value.find((skill) => skill.id === id) ?? null
  }

  return { skills, loading, error, loadedAt, load, refresh, getDetail, findById }
})
