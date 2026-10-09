import './styles/global.css';
import App from './App.svelte';
import { setupInputLayout } from '$lib/layout';
import { mount, unmount } from 'svelte';
import { bridge, setupAutoMarkDirty } from '$lib/bridge';

const app = mount(App, {
    target: document.getElementById('app')!,
});

// Auto-mark UI as dirty when DOM changes
const teardownAutoMarkDirty = setupAutoMarkDirty();
const teardownInputLayout = setupInputLayout(document.getElementById('app')!);

if (import.meta.hot) {
    import.meta.hot.dispose(() => {
        teardownAutoMarkDirty();
        teardownInputLayout();
        bridge.dispose();
        void unmount(app);
    });
}

export default app;
