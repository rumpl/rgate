import init, { start } from './pkg/rgate_web.js';

const status = document.querySelector('#status');
const showError = error => {
    status.hidden = false;
    status.textContent = `RGate could not start:\n${error?.message ?? error}\n\nUse a recent browser with WebGPU or WebGL2 enabled.\nWebGPU requires HTTPS or localhost.`;
    console.error(error);
};
window.addEventListener('error', event => showError(event.error ?? event.message));
window.addEventListener('unhandledrejection', event => showError(event.reason));

try {
    await init();
    start();
    const started = new Promise(resolve => {
        const poll = () => {
            if (document.querySelector('canvas')) { resolve(); return; }
            requestAnimationFrame(poll);
        };
        poll();
    });
    await Promise.race([started, new Promise((_, reject) => setTimeout(() => reject(new Error('Graphics initialization timed out. See the browser console for details.')), 30000))]);
    status.hidden = true;
    document.body.dataset.rgateReady = 'true';
} catch (error) {
    showError(error);
}
