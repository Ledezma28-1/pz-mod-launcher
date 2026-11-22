import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { logUiError } from './lib/api';

addEventListener('error', (e) => logUiError(e.error ?? e.message));
addEventListener('unhandledrejection', (e) => logUiError(e.reason));

const app = mount(App, { target: document.getElementById('app')! });

export default app;
