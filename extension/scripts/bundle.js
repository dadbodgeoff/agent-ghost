#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const rootDir = path.resolve(__dirname, '..');
const distDir = path.join(rootDir, 'dist');

function ensureDir(dirPath) {
  fs.mkdirSync(dirPath, { recursive: true });
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function rewriteManifestPaths(manifest) {
  const rewritten = structuredClone(manifest);

  if (rewritten.background?.service_worker) {
    rewritten.background.service_worker = rewritten.background.service_worker.replace(/^src\//, '');
  }

  if (Array.isArray(rewritten.background?.scripts)) {
    rewritten.background.scripts = rewritten.background.scripts.map((entry) =>
      entry.replace(/^src\//, ''),
    );
  }

  if (Array.isArray(rewritten.content_scripts)) {
    rewritten.content_scripts = rewritten.content_scripts.map((script) => ({
      ...script,
      js: Array.isArray(script.js)
        ? script.js.map((entry) => entry.replace(/^src\//, ''))
        : script.js,
    }));
  }

  if (rewritten.action?.default_popup) {
    rewritten.action.default_popup = rewritten.action.default_popup.replace(/^src\//, '');
  }

  if (rewritten.browser_action?.default_popup) {
    rewritten.browser_action.default_popup = rewritten.browser_action.default_popup.replace(
      /^src\//,
      '',
    );
  }

  const iconsDir = path.join(rootDir, 'icons');
  if (!fs.existsSync(iconsDir)) {
    if (rewritten.action?.default_icon) {
      delete rewritten.action.default_icon;
    }
    if (rewritten.icons) {
      delete rewritten.icons;
    }
  }

  return rewritten;
}

function copyFile(relativePath) {
  const sourcePath = path.join(rootDir, relativePath);
  const targetPath = path.join(distDir, relativePath.replace(/^src\//, ''));
  ensureDir(path.dirname(targetPath));
  fs.copyFileSync(sourcePath, targetPath);
}

function main() {
  ensureDir(distDir);

  const chromeManifest = rewriteManifestPaths(
    readJson(path.join(rootDir, 'manifest.chrome.json')),
  );
  const firefoxManifest = rewriteManifestPaths(
    readJson(path.join(rootDir, 'manifest.firefox.json')),
  );

  writeJson(path.join(distDir, 'manifest.json'), chromeManifest);
  writeJson(path.join(distDir, 'manifest.firefox.json'), firefoxManifest);

  copyFile('src/popup/popup.html');
  copyFile('src/dashboard/index.html');

  const iconsDir = path.join(rootDir, 'icons');
  if (fs.existsSync(iconsDir)) {
    fs.cpSync(iconsDir, path.join(distDir, 'icons'), { recursive: true });
  }
}

main();
