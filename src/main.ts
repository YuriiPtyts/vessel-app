import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
import './lib/themes/duskfox.css';
import './lib/glass.css';
import './lib/home.css';
import './lib/controls.css';
import './lib/readability.css';
import './lib/features.css';
import './lib/providers.css';

const target = document.getElementById('app');

if (!target) {
  throw new Error('The application mount point is missing.');
}

mount(App, { target });
