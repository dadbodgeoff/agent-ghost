import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		port: 39781,
		strictPort: true, // Tauri devUrl expects exactly this port
		host: '0.0.0.0',
	},
	envPrefix: ['VITE_', 'TAURI_'],
});
