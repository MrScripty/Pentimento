import './styles/global.css';
import App from './App.svelte';
import { mount, unmount } from 'svelte';
import { bridge, setupAutoMarkDirty } from '$lib/bridge';

const app = mount(App, {
    target: document.getElementById('app')!,
});

// Auto-mark UI as dirty when DOM changes
const teardownAutoMarkDirty = setupAutoMarkDirty();

if (import.meta.hot) {
    import.meta.hot.dispose(() => {
        teardownAutoMarkDirty();
        bridge.dispose();
        void unmount(app);
    });
}

export default app;
