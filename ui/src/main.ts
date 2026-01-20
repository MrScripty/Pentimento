import './styles/global.css';
import App from './App.svelte';
import { mount } from 'svelte';
import { setupAutoMarkDirty } from '$lib/bridge';

const app = mount(App, {
    target: document.getElementById('app')!,
});

// Auto-mark UI as dirty when DOM changes
setupAutoMarkDirty();

export default app;
