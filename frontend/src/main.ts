import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

// Disable zooming globally on both desktop and mobile
window.addEventListener(
  'wheel',
  (e: WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
    }
  },
  { passive: false }
);

window.addEventListener('keydown', (e: KeyboardEvent) => {
  if (
    (e.ctrlKey || e.metaKey) &&
    (e.key === '=' ||
      e.key === '-' ||
      e.key === '+' ||
      e.key === '0' ||
      e.key === '_' ||
      e.code === 'NumpadAdd' ||
      e.code === 'NumpadSubtract')
  ) {
    e.preventDefault();
  }
});

// Disable gesture zooming (Safari/WebKit on macOS/iOS)
document.addEventListener('gesturestart', (e) => e.preventDefault());
document.addEventListener('gesturechange', (e) => e.preventDefault());
document.addEventListener('gestureend', (e) => e.preventDefault());

// Prevent multi-touch pinch gestures
document.addEventListener(
  'touchmove',
  (e: TouchEvent) => {
    if (e.touches.length > 1) {
      e.preventDefault();
    }
  },
  { passive: false }
);

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app

