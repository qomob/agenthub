import { createPinia } from 'pinia'
import { createApp } from 'vue'

import App from './App.vue'
import { i18n } from './i18n'
import { router } from './router'
import './styles.css'

const app = createApp(App)

app.use(createPinia())
app.use(router)
app.use(i18n)

app.config.errorHandler = (error, _instance, info) => {
  // Keep unexpected render/runtime errors visible in the console; command errors
  // are surfaced by the calling page with a readable banner or toast.
  console.error(`[AgentHub] ${info}`, error)
}

app.mount('#app')
