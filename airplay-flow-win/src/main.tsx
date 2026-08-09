import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App'

const rootEl = document.getElementById('root')

if (!rootEl) {
  // Fallback: if React can't mount, at least show something
  document.body.innerHTML = '<div style="color:white;padding:40px;font-family:sans-serif"><h1>AirPlay Flow Win</h1><p>Root element not found. Check index.html.</p></div>'
} else {
  try {
    createRoot(rootEl).render(
      <StrictMode>
        <App />
      </StrictMode>,
    )
  } catch (e) {
    rootEl.innerHTML = `<div style="color:#ef4444;padding:40px;font-family:sans-serif"><h1>App Error</h1><pre>${String(e)}</pre></div>`
  }
}
