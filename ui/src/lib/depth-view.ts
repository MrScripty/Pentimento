import type { BevyToUi, DepthViewState } from './types';

interface DepthViewBridge {
    subscribe(handler: (message: BevyToUi) => void): () => void;
    getDepthViewState(): void;
    setDepthView(enabled: boolean): void;
}

export interface DepthViewController {
    toggle(): void;
    dispose(): void;
}

export function initialDepthViewState(): DepthViewState {
    return {
        available: false,
        enabled: false,
        reason: 'Checking depth view availability…',
    };
}

/** Keep display state authoritative, including startup and late-mounted UIs. */
export function connectDepthView(
    bridge: DepthViewBridge,
    onState: (state: DepthViewState) => void,
): DepthViewController {
    let state = initialDepthViewState();
    let active = true;
    onState(state);

    const unsubscribe = bridge.subscribe((message) => {
        if (!active) return;

        if (message.type === 'DepthViewState') {
            state = {
                ...message.data,
                enabled: message.data.available && message.data.enabled,
            };
            onState(state);
        } else if (message.type === 'DepthViewRejected') {
            state = { ...state, reason: message.data.reason };
            onState(state);
        }
    });

    // Subscribe first: native IPC may answer synchronously. The query also
    // recovers a startup broadcast sent before the toolbar mounted.
    bridge.getDepthViewState();

    return {
        toggle() {
            if (active && state.available) {
                bridge.setDepthView(!state.enabled);
            }
        },
        dispose() {
            active = false;
            unsubscribe();
        },
    };
}
