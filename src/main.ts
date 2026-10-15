import { createApp } from 'vue'
import App from './App.vue'
import { use_theme } from './composables/use_theme'
import './styles.css'


use_theme().theme_start()
createApp(App).mount('#app')
