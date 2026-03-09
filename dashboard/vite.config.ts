import { readFileSync } from 'node:fs';

import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const packageJson = JSON.parse(
	readFileSync(new URL('./package.json', import.meta.url), 'utf8'),
) as { version: string };

export default defineConfig({
	plugins: [sveltekit()],
	define: {
		__APP_VERSION__: JSON.stringify(packageJson.version),
	},
	build: {
		chunkSizeWarningLimit: 700,
		rollupOptions: {
			output: {
				manualChunks(id) {
					if (!id.includes('node_modules')) return;
					if (id.includes('@xterm')) return 'vendor-xterm';
					if (id.includes('@codemirror')) return 'vendor-codemirror';
					if (id.includes('highlight.js') || id.includes('marked') || id.includes('dompurify')) {
						return 'vendor-markdown';
					}
					if (
						id.includes('d3-force') ||
						id.includes('d3-selection') ||
						id.includes('d3-zoom') ||
						id.includes('d3-drag') ||
						id.includes('d3-scale')
					) {
						return 'vendor-d3';
					}
					if (id.includes('@tauri-apps')) return 'vendor-tauri';
					if (id.includes('@ghost/sdk')) return 'vendor-ghost-sdk';
				},
			},
		},
	},
	server: {
		port: 39781,
		strictPort: true, // Tauri devUrl expects exactly this port
		host: '0.0.0.0',
	},
	envPrefix: ['VITE_', 'TAURI_'],
});
