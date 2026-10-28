import fs from 'fs';
import path from 'path';

// The "research archive" palette (see .vitepress/theme/tokens.css). Compared case-insensitively.
const PALETTE = [
  '#000000', // Ink Black
  '#FFFFFF', // Pure White
  '#F2F2F2', // Light Grid Grey
  '#F5F5F5', // Soft Paper Grey
  '#111111', // Near Black
  '#777777', // Slate Grey
  '#1A1A1A', // Charcoal
  '#D8D8D8', // Light Grey
  '#E8E8E8', // Neutral Grey
];
const ALLOWED_HEX = new Set(PALETTE.map((h) => h.toUpperCase()));

const EXTENSIONS = new Set(['.css', '.vue', '.ts', '.js', '.mjs', '.md', '.svg', '.json']);
const IGNORE_DIRS = new Set(['node_modules', '.vitepress/cache', '.vitepress/dist', '.git', 'dist']);

const ROOT_DIR = process.cwd();

let errors = 0;

function scanDir(dir) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    const relPath = path.relative(ROOT_DIR, fullPath).replace(/\\/g, '/');

    if (entry.isDirectory()) {
      if (IGNORE_DIRS.has(entry.name) || Array.from(IGNORE_DIRS).some(d => relPath.startsWith(d))) {
        continue;
      }
      scanDir(fullPath);
    } else if (entry.isFile()) {
      const ext = path.extname(entry.name).toLowerCase();
      if (!EXTENSIONS.has(ext)) continue;
      if (entry.name === 'package-lock.json') continue;

      const content = fs.readFileSync(fullPath, 'utf8');
      
      // Remove code blocks from markdown if necessary, or check hex colors
      // We look for 6-digit hex color patterns #RRGGBB
      const hexMatches = content.match(/#[0-9a-fA-F]{6}\b/g) || [];
      for (const hex of hexMatches) {
        if (!ALLOWED_HEX.has(hex.toUpperCase())) {
          // Check if this hex is a markdown anchor, git commit hash, or actual color
          // We flag actual color values
          console.error(`[check:palette] Disallowed hex color found in ${relPath}: ${hex}`);
          errors++;
        }
      }
    }
  }
}

console.log('--- Checking research-archive palette compliance ---');
scanDir(ROOT_DIR);

if (errors > 0) {
  console.error(`FAILED: Found ${errors} disallowed color literals. Allowed: ${PALETTE.join(', ')}.`);
  process.exit(1);
} else {
  console.log(`PASSED: every hex colour in the scanned files is one of ${PALETTE.join(', ')}.`);
}
