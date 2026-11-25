import * as esbuild from 'esbuild';
import fs from 'node:fs';
import path from 'node:path';

const IS_PROD = process.argv.includes('--prod') || process.env.NODE_ENV === 'production';

const distDir = path.resolve('dist');
fs.rmSync(distDir, { recursive: true, force: true });
fs.mkdirSync(path.join(distDir, 'main'), { recursive: true });
fs.mkdirSync(path.join(distDir, 'preload'), { recursive: true });
fs.mkdirSync(path.join(distDir, 'renderer'), { recursive: true });

await Promise.all([
  esbuild.build({
    entryPoints: ['src/main/main.ts'],
    bundle: true,
    platform: 'node',
    target: 'node22',
    format: 'cjs',
    outfile: 'dist/main/main.js',
    external: ['electron', 'electron-updater'],
    minify: IS_PROD,
    sourcemap: !IS_PROD,
  }),
  esbuild.build({
    entryPoints: ['src/main/mcp-computer-use.ts'],
    bundle: true,
    platform: 'node',
    target: 'node22',
    format: 'cjs',
    outfile: 'dist/main/mcp-computer-use.js',
    minify: IS_PROD,
    sourcemap: !IS_PROD,
  }),
  esbuild.build({
    entryPoints: ['src/preload/preload.ts'],
    bundle: true,
    platform: 'node',
    target: 'node22',
    format: 'cjs',
    outfile: 'dist/preload/preload.js',
    external: ['electron'],
    minify: IS_PROD,
    sourcemap: !IS_PROD,
  }),
  esbuild.build({
    entryPoints: ['src/renderer/app.ts'],
    bundle: true,
    platform: 'browser',
    target: 'chrome132',
    format: 'esm',
    splitting: true,
    outdir: 'dist/renderer',
    loader: {
      '.ttf': 'file',
      '.woff': 'file',
      '.woff2': 'file',
      '.eot': 'file',
      '.svg': 'file',
    },
    minify: IS_PROD,
    sourcemap: !IS_PROD,
  }),
  esbuild.build({
    entryPoints: ['src/renderer/markup-app.ts'],
    bundle: true,
    platform: 'browser',
    target: 'chrome132',
    format: 'esm',
    outfile: 'dist/renderer/markup-app.js',
    minify: IS_PROD,
    sourcemap: !IS_PROD,
  }),
]);

// Copy static HTML + CSS + icon assets into dist/renderer
fs.copyFileSync('src/renderer/index.html', 'dist/renderer/index.html');
fs.copyFileSync('src/renderer/markup.html', 'dist/renderer/markup.html');
fs.copyFileSync('src/renderer/styles.css', 'dist/renderer/styles.css');
fs.copyFileSync('src/renderer/markup.css', 'dist/renderer/markup.css');

// Copy application icons and resources into dist/resources
if (fs.existsSync('resources')) {
  fs.cpSync('resources', 'dist/resources', { recursive: true });
}

// Bundle KaTeX CSS + fonts and Codicons CSS + fonts locally so zero CDN is needed
const katexDist = path.resolve('node_modules/katex/dist');
if (fs.existsSync(katexDist)) {
  fs.cpSync(path.join(katexDist, 'katex.min.css'), 'dist/renderer/katex.min.css');
  fs.cpSync(path.join(katexDist, 'fonts'), 'dist/renderer/fonts', { recursive: true });
}
const codiconDist = path.resolve('node_modules/@vscode/codicons/dist');
if (fs.existsSync(codiconDist)) {
  fs.cpSync(path.join(codiconDist, 'codicon.css'), 'dist/renderer/codicon.css');
  fs.cpSync(path.join(codiconDist, 'codicon.ttf'), 'dist/renderer/codicon.ttf');
}

console.log('Build complete -> dist/');
