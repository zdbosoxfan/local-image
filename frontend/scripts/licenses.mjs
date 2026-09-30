import fs from 'node:fs';
import path from 'node:path';

const lock = JSON.parse(fs.readFileSync('package-lock.json', 'utf8'));
const notices = ['Local Image frontend — resolved runtime dependency notices',
  'Generated from the committed package-lock.json. System fonts are referenced, not redistributed.',
  'No remote UI/font assets are required. Development-only dependencies are omitted.'];
for (const [location, metadata] of Object.entries(lock.packages).sort(([a], [b]) => a.localeCompare(b))) {
  if (!location || metadata.dev || metadata.devOptional) continue;
  const packagePath = path.resolve(location);
  const pkg = JSON.parse(fs.readFileSync(path.join(packagePath, 'package.json'), 'utf8'));
  const files = fs.readdirSync(packagePath).filter(name => /^(licen[sc]e|notice)(\.|$)/i.test(name) && fs.statSync(path.join(packagePath, name)).isFile());
  const iconFallback = pkg.name === '@fluentui/react-icons' && pkg.version === '2.0.343';
  const emblaFallback = ['embla-carousel', 'embla-carousel-autoplay', 'embla-carousel-fade'].includes(pkg.name) && pkg.version === '8.6.0';
  if (!files.length && !iconFallback && !emblaFallback) throw new Error(`Missing license text for ${pkg.name}; review before packaging.`);
  notices.push(`\n${'='.repeat(72)}\n${pkg.name} ${pkg.version}\nDeclared license: ${pkg.license}\nResolved: ${metadata.resolved ?? 'npm registry'}`);
  for (const file of files) notices.push(`\n--- ${file} ---\n${fs.readFileSync(path.join(packagePath, file), 'utf8')}`);
  if (iconFallback && !files.length) {
    notices.push(fs.readFileSync('licenses/fluentui-system-icons-provenance.json', 'utf8'));
    notices.push(fs.readFileSync('licenses/fluentui-system-icons-LICENSE.txt', 'utf8'));
  }
  if (emblaFallback && !files.length) {
    notices.push(fs.readFileSync('licenses/embla-carousel-provenance.json', 'utf8'));
    notices.push(fs.readFileSync('licenses/embla-carousel-LICENSE.txt', 'utf8'));
  }
}
fs.writeFileSync('../backend/frontend_dist/THIRD_PARTY_NOTICES.txt', notices.join('\n\n').trimEnd() + '\n');
console.log('Wrote frontend runtime license texts to backend/frontend_dist/THIRD_PARTY_NOTICES.txt');
