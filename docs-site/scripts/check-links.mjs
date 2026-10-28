import fs from 'fs';
import path from 'path';

const ROOT_DIR = process.cwd();
const IGNORE_DIRS = new Set(['node_modules', '.vitepress/cache', '.vitepress/dist', '.git', 'dist']);

const markdownFiles = [];

function collectMarkdown(dir) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    const relPath = path.relative(ROOT_DIR, fullPath).replace(/\\/g, '/');

    if (entry.isDirectory()) {
      if (IGNORE_DIRS.has(entry.name) || Array.from(IGNORE_DIRS).some(d => relPath.startsWith(d))) {
        continue;
      }
      collectMarkdown(fullPath);
    } else if (entry.isFile() && entry.name.endsWith('.md')) {
      markdownFiles.push({ fullPath, relPath });
    }
  }
}

collectMarkdown(ROOT_DIR);

let brokenLinks = 0;

// 1. Check all links in .vitepress/config.ts
const configContent = fs.readFileSync(path.join(ROOT_DIR, '.vitepress/config.ts'), 'utf8');
const configLinks = [];
const configRegex = /link:\s*'([^']+)'/g;
let m;
while ((m = configRegex.exec(configContent)) !== null) {
  configLinks.push(m[1]);
}

for (const link of configLinks) {
  let target = link.split('#')[0];
  if (target.endsWith('/')) {
    target = target + 'index.md';
  } else {
    target = target + '.md';
  }
  target = target.replace(/^\//, '');
  if (!fs.existsSync(path.join(ROOT_DIR, target))) {
    console.error(`[check:links] config.ts references missing file: ${link} -> ${target}`);
    brokenLinks++;
  }
}

// 2. Check internal links inside all markdown files
for (const { fullPath, relPath } of markdownFiles) {
  const content = fs.readFileSync(fullPath, 'utf8');
  // Match markdown links: [text](/path/to/page) or [text](/path/to/page#anchor)
  const linkMatches = content.match(/\[[^\]]+\]\((\/[^\)#\s]+)(?:#[^\)]*)?\)/g) || [];
  for (const match of linkMatches) {
    const urlMatch = match.match(/\[[^\]]+\]\((\/[^\)#\s]+)/);
    if (!urlMatch) continue;
    let url = urlMatch[1];
    let target = url;
    if (target.endsWith('/')) {
      target = target + 'index.md';
    } else {
      target = target + '.md';
    }
    target = target.replace(/^\//, '');
    if (!fs.existsSync(path.join(ROOT_DIR, target))) {
      console.error(`[check:links] ${relPath} contains broken link: ${url} -> ${target}`);
      brokenLinks++;
    }
  }
}

console.log('--- Checking Documentation Links ---');
console.log(`Scanned ${markdownFiles.length} markdown documents and ${configLinks.length} config navigation links.`);

if (brokenLinks > 0) {
  console.error(`FAILED: Found ${brokenLinks} broken internal links.`);
  process.exit(1);
} else {
  console.log('PASSED: All internal navigation and content links resolve successfully.');
}
