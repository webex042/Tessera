import fs from 'fs';
import path from 'path';

const ALLOWED_HEX = new Set([
  '#0A0A0A',
  '#0a0a0a',
  '#007BA7',
  '#007ba7',
  '#FF2D95',
  '#ff2d95',
]);

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
        if (!ALLOWED_HEX.has(hex)) {
          // Check if this hex is a markdown anchor, git commit hash, or actual color
          // We flag actual color values
          console.error(`[check:palette] Disallowed hex color found in ${relPath}: ${hex}`);
          errors++;
        }
      }
    }
  }
}

console.log('--- Checking Palette A Compliance ---');
scanDir(ROOT_DIR);

if (errors > 0) {
  console.error(`FAILED: Found ${errors} disallowed color literals. Palette A allows only #0A0A0A, #007BA7, and #FF2D95.`);
  process.exit(1);
} else {
  console.log('PASSED: All scanned files strictly adhere to Palette A (#0A0A0A, #007BA7, #FF2D95).');
}
