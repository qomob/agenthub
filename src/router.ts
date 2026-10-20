import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'

import { i18n } from './i18n'
import ActivityView from './pages/ActivityView.vue'
import AgentsView from './pages/AgentsView.vue'
import DashboardView from './pages/DashboardView.vue'
import MatrixView from './pages/MatrixView.vue'
import McpView from './pages/McpView.vue'
import PluginsView from './pages/PluginsView.vue'
import ProfilesView from './pages/ProfilesView.vue'
import SettingsView from './pages/SettingsView.vue'
import SkillDetailView from './pages/SkillDetailView.vue'
import SkillsView from './pages/SkillsView.vue'

export interface NavItem {
  path: string
  name: string
  labelKey: string
  icon: string
  shortcut?: string
  group: 'overview' | 'capabilities' | 'workspace' | 'system'
}

/** Shared by the sidebar and the Cmd+1..9 shortcuts (order matters). */
export const navItems: NavItem[] = [
  {
    path: '/dashboard',
    name: 'dashboard',
    labelKey: 'nav.dashboard',
    icon: 'dashboard',
    shortcut: '1',
    group: 'overview',
  },
  {
    path: '/agents',
    name: 'agents',
    labelKey: 'nav.agents',
    icon: 'agents',
    shortcut: '2',
    group: 'capabilities',
  },
  {
    path: '/skills',
    name: 'skills',
    labelKey: 'nav.skills',
    icon: 'skills',
    shortcut: '3',
    group: 'capabilities',
  },
  {
    path: '/matrix',
    name: 'matrix',
    labelKey: 'nav.matrix',
    icon: 'matrix',
    shortcut: '4',
    group: 'capabilities',
  },
  {
    path: '/mcp',
    name: 'mcp',
    labelKey: 'nav.mcp',
    icon: 'mcp',
    shortcut: '5',
    group: 'capabilities',
  },
  {
    path: '/plugins',
    name: 'plugins',
    labelKey: 'nav.plugins',
    icon: 'plugins',
    shortcut: '6',
    group: 'capabilities',
  },
  {
    path: '/profiles',
    name: 'profiles',
    labelKey: 'nav.profiles',
    icon: 'profiles',
    shortcut: '7',
    group: 'workspace',
  },
  {
    path: '/activity',
    name: 'activity',
    labelKey: 'nav.activity',
    icon: 'activity',
    shortcut: '8',
    group: 'workspace',
  },
  {
    path: '/settings',
    name: 'settings',
    labelKey: 'nav.settings',
    icon: 'settings',
    shortcut: '9',
    group: 'system',
  },
]

const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/dashboard' },
  {
    path: '/dashboard',
    name: 'dashboard',
    component: DashboardView,
    meta: { titleKey: 'nav.dashboard' },
  },
  {
    path: '/agents',
    name: 'agents',
    component: AgentsView,
    meta: { titleKey: 'nav.agents' },
  },
  {
    path: '/skills',
    name: 'skills',
    component: SkillsView,
    meta: { titleKey: 'nav.skills' },
  },
  {
    path: '/skills/:id',
    name: 'skill-detail',
    component: SkillDetailView,
    meta: { titleKey: 'nav.skills' },
  },
  {
    path: '/matrix',
    name: 'matrix',
    component: MatrixView,
    meta: { titleKey: 'nav.matrix' },
  },
  { path: '/mcp', name: 'mcp', component: McpView, meta: { titleKey: 'nav.mcp' } },
  {
    path: '/plugins',
    name: 'plugins',
    component: PluginsView,
    meta: { titleKey: 'nav.plugins' },
  },
  {
    path: '/profiles',
    name: 'profiles',
    component: ProfilesView,
    meta: { titleKey: 'nav.profiles' },
  },
  {
    path: '/activity',
    name: 'activity',
    component: ActivityView,
    meta: { titleKey: 'nav.activity' },
  },
  {
    path: '/settings',
    name: 'settings',
    component: SettingsView,
    meta: { titleKey: 'nav.settings' },
  },
  { path: '/:pathMatch(.*)*', redirect: '/dashboard' },
]

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
  scrollBehavior: () => ({ top: 0 }),
})

router.afterEach((to) => {
  const key = typeof to.meta.titleKey === 'string' ? to.meta.titleKey : 'common.appName'
  document.title = `${i18n.global.t(key)} · AgentHub`
})
