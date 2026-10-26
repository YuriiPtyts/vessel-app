import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
import './lib/themes/duskfox.css';
import './lib/glass.css';
import './lib/home.css';

const target = document.getElementById('app');

if (!target) {
  throw new Error('The application mount point is missing.');
}

mount(App, { target });
