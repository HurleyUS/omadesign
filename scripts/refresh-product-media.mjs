import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const output = path.join(root, 'artifacts/media-refresh');
const binary = process.env.OMADESIGN_BIN || path.join(output, 'omadesign-capture');
const recorder = process.env.OMADESIGN_RECORDER || path.join(process.env.CARGO_TARGET_DIR || path.join(root, 'target'), 'debug/capture_studios');
const profile = path.join(output, 'profile');
const env = { ...process.env, TMPDIR: process.env.TMPDIR || path.join(root, '.t') };
fs.mkdirSync(env.TMPDIR, { recursive: true });
const home = path.join(output, 'home');
const project = path.join(home, 'Work/Fieldwork');
fs.mkdirSync(project, { recursive: true });
for (const [source, name] of [['design', 'Make space'], ['pixel', 'Brush studies'], ['motion', 'Room to move']]) fs.copyFileSync(path.join(root, `examples/product-media/${source}.oma`), path.join(project, `${name}.oma`));
fs.cpSync(path.join(root, 'examples/fieldwork/.omabrand'), path.join(project, '.omabrand'), { recursive: true });
env.HOME = home;
delete env.DISPLAY;
delete env.SWAYSOCK;
for (const [key, folder] of Object.entries({ XDG_CONFIG_HOME: 'config', XDG_DATA_HOME: 'data', XDG_CACHE_HOME: 'cache', XDG_STATE_HOME: 'state' })) {
  env[key] = path.join(profile, folder);
  fs.mkdirSync(env[key], { recursive: true });
}
if (!env.XDG_RUNTIME_DIR?.startsWith('/home/') || !env.WAYLAND_DISPLAY) throw new Error('Set XDG_RUNTIME_DIR and WAYLAND_DISPLAY to an isolated display under /home.');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const receipts = [];
function run(command, args, name) {
  console.log(name);
  const log = fs.openSync(path.join(output, `${name}.log`), 'w');
  const result = spawnSync(command, args, { cwd: root, env, stdio: ['ignore', log, log] });
  fs.closeSync(log);
  if (result.status !== 0) throw new Error(`${name} failed (${result.status}); see ${output}/${name}.log`);
}
function copy(source, destination) {
  fs.mkdirSync(path.dirname(path.join(root, destination)), { recursive: true });
  fs.copyFileSync(source, path.join(root, destination));
  receipts.push({ file: destination, sha256: hash(source), source: path.relative(root, source) });
}
function image(source, destination, extra = []) {
  fs.mkdirSync(path.dirname(path.join(root, destination)), { recursive: true });
  run('ffmpeg', ['-y', '-loglevel', 'error', '-i', source, ...extra, '-frames:v', '1', path.join(root, destination)], `encode-${destination.replaceAll('/', '-')}`);
  receipts.push({ file: destination, sha256: hash(path.join(root, destination)), source: path.relative(root, source) });
}
function shot(id, input) {
  const result = path.join(output, `${id}-native.png`);
  if (process.env.MEDIA_RESUME === '1' && fs.existsSync(result)) return result;
  run(binary, [input.endsWith('.oma') ? '--shot-file' : '--shot', input, '--size', '1600x900', '--out', result], `capture-${id}`);
  return result;
}
const phase = process.argv[2] || 'all';
if (!['all', 'capture', 'publish'].includes(phase)) throw new Error('Use all, capture, or publish.');
const clips = [
  ['design', 'media-design', 12, 'Select and move the editable arrow; undo and redo.'],
  ['pixel', 'media-pixel', 12, 'Paint with the angled Square brush; undo and redo.'],
  ['motion', 'media-motion', 12, 'Play and pause the editable pulse animation.'],
  ['agent-panel', 'media-agent', 12, 'Compose a brief in the native Agent panel. No provider response is staged.'],
  ['graphics', 'graphics', 17, 'Edit gradient placement and direction; group and undo.'],
  ['layout', 'layout', 15, 'Present the responsive layout; draw a frame and undo.'],
  ['chroma', 'chroma', 15, 'Compare, sample, apply and undo chroma key.'],
  ['photo', 'photo', 24, 'Develop the CC0 camera photograph: light, color, detail and comparison.'],
  ['brand-kit', 'brand-kit', 30, 'Browse portable palette, asset and typography libraries.'],
  ['brush-controls', 'media-brush-controls', 10, 'Set spacing to 10,000 percent; enable direction and adjust scatter and all jitter controls.'],
];
if (phase === 'all' || phase === 'capture') {
  for (const [id, scene] of clips) {
    const directory = path.join(output, 'takes', id);
    fs.mkdirSync(directory, { recursive: true });
    const errorFile = path.join(directory, `${scene}-errors.json`);
    if (process.env.MEDIA_RESUME === '1' && fs.existsSync(errorFile) && JSON.parse(fs.readFileSync(errorFile)).length === 0) continue;
    run(recorder, [scene, directory, '--fps', scene === 'media-brush-controls' ? '15' : '30'], `record-${id}`);
    const errors = JSON.parse(fs.readFileSync(path.join(directory, `${scene}-errors.json`)));
    if (errors.length) throw new Error(`${id}: ${errors.join('; ')}`);
  }
}
if (phase === 'all' || phase === 'publish') {
  for (const [id, scene] of clips) {
    const directory = path.join(output, 'takes', id);
    const errors = JSON.parse(fs.readFileSync(path.join(directory, `${scene}-errors.json`)));
    if (errors.length) throw new Error(`${id}: unresolved native actions`);
  }
  const screenshots = {};
  for (const id of ['design', 'pixel', 'type', 'motion']) screenshots[id] = shot(id, `examples/product-media/${id}.oma`);
  for (const id of ['design-tools', 'layout', 'layout-phone', 'hud-pen', 'templates', 'welcome', 'pixel-effects']) screenshots[id] = shot(id, id);
  screenshots.photo = path.join(output, 'takes/photo/photo-0000.png');
  screenshots['brand-library'] = path.join(output, 'takes/brand-kit/brand-kit-0000.png');
  screenshots['brush-controls'] = path.join(output, 'takes/brush-controls/media-brush-controls-0120.png');
  for (const [id, source] of Object.entries(screenshots)) {
    image(source, `site/public/media/refresh/${id}.webp`, ['-c:v', 'libwebp', '-lossless', '1']);
    copy(source, `media/shots/${id === 'design' ? 'design' : id}.png`);
  }
  copy(screenshots.design, 'docs/media/studio.png');
  copy(screenshots['brush-controls'], 'docs/media/brush-controls.png');
  copy(path.join(root, 'examples/product-media/pixel.oma'), 'site/public/brushes/studies.oma');
  copy(screenshots.welcome, 'media/welcome.png');
  image(screenshots.motion, 'site/public/media/refresh/motion-presets.webp', ['-c:v', 'libwebp', '-lossless', '1']);
  for (const [id, sourceId] of [['design', 'design'], ['paint', 'pixel'], ['photo', 'photo'], ['type', 'type'], ['assets', 'brand-library']]) {
    for (const folder of ['media', 'remotion/public/media', ...(id === 'assets' || id === 'shapes' ? [] : ['docs/media'])]) image(screenshots[sourceId], `${folder}/${id}.jpg`, ['-q:v', '2']);
  }
  for (const [id, scene, duration, description] of clips) {
    const source = path.join(output, 'takes', id, `${scene}.mp4`);
    const base = id === 'agent-panel' ? 'site/public/media/ai/agent-panel' : `site/public/media/recordings/${id}`;
    copy(source, `${base}.mp4`);
    run('ffmpeg', ['-y', '-loglevel', 'error', '-i', source, '-c:v', 'libvpx-vp9', '-b:v', '0', '-crf', '32', '-row-mt', '1', '-threads', '4', '-an', path.join(root, `${base}.webm`)], `webm-${id}`);
    image(source, `${base}.webp`, ['-c:v', 'libwebp', '-lossless', '1']);
    fs.writeFileSync(path.join(root, `${base}.vtt`), `WEBVTT\n\n00:00:00.000 --> 00:00:${String(duration).padStart(2, '0')}.000\n${description}\n`);
    run('ffmpeg', ['-v', 'error', '-i', source, '-f', 'null', '-'], `decode-${id}`);
  }
  const ending = path.join(output, 'logo-ending.png');
  const lockup = fs.readFileSync(path.join(root, 'docs/media/omadesign-logo-light.svg'), 'utf8').replace(/^.*?<svg[^>]*>/s, '').replace(/<\/svg>\s*$/, '');
  const endingSvg = path.join(output, 'logo-ending.svg');
  fs.writeFileSync(endingSvg, `<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="900" viewBox="0 0 1600 900"><rect width="1600" height="900" fill="#f4f1e8"/><svg x="200" y="310" width="1200" height="194" viewBox="0 0 744 120">${lockup}</svg><text x="418" y="565" fill="#153d34" font-family="Liberation Sans" font-size="25" letter-spacing="4">DESIGN. PAINT. DEVELOP. ANIMATE.</text></svg>`);
  run(binary, ['--convert', endingSvg, '--output', ending], 'export-logo-ending');
  const film = path.join(root, 'site/public/media/studio/film-current.mp4');
  const sequence = ['design', 'layout', 'graphics', 'pixel', 'chroma', 'photo', 'motion', 'brand-kit', 'agent-panel', 'design'];
  const inputs = sequence.flatMap(id => ['-threads', '1', '-i', path.join(output, 'takes', id, `${clips.find(c => c[0] === id)[1]}.mp4`)]).concat(['-loop', '1', '-framerate', '30', '-i', ending]);
  const crops = [[160,190,960,540],[580,80,960,540],[800,140,800,450],[720,405,880,495],[400,180,960,540],[640,160,960,540],[320,180,1280,720],[800,80,800,450],[640,360,960,540]];
  const cuts = [];
  const parts = [];
  for (let i = 0; i < 9; i++) {
    const [x,y,w,h] = crops[i];
    parts.push(`[${i}:v]split=2[a${i}][b${i}];[a${i}]trim=start=1:end=3,setpts=PTS-STARTPTS,setsar=1[w${i}];[b${i}]trim=start=3:end=5,setpts=PTS-STARTPTS,crop=${w}:${h}:${x}:${y},scale=1600:900,setsar=1[c${i}]`);
    cuts.push(`[w${i}][c${i}]`);
  }
  parts.push('[9:v]trim=start=1:end=3,setpts=PTS-STARTPTS,setsar=1[last];[10:v]trim=duration=2,setpts=PTS-STARTPTS,setsar=1[logo]');
  const filters = parts.join(';') + ';' + cuts.join('') + '[last][logo]concat=n=20:v=1:a=0[out]';
  run('ffmpeg', ['-y', '-loglevel', 'error', ...inputs, '-filter_complex_threads', '1', '-filter_complex', filters, '-map', '[out]', '-c:v', 'libx264', '-preset', 'fast', '-crf', '21', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', '-an', film], 'cut-film');
  run('ffmpeg', ['-y', '-loglevel', 'error', '-i', film, '-c:v', 'libvpx-vp9', '-b:v', '0', '-crf', '32', '-row-mt', '1', '-threads', '4', '-an', film.replace('.mp4', '.webm')], 'webm-film');
  image(screenshots.design, 'site/public/media/studio/film-current.webp', ['-c:v', 'libwebp', '-lossless', '1']);
  fs.writeFileSync(film.replace('.mp4', '.vtt'), 'WEBVTT\n\n' + sequence.map((id, i) => `00:00:${String(i * 4).padStart(2, '0')}.000 --> 00:00:${String(i === 9 ? 38 : (i + 1) * 4).padStart(2, '0')}.000\n${clips.find(c => c[0] === id)[3]}\n`).join('\n') + '\n00:00:38.000 --> 00:00:40.000\nThe canonical Omadesign logo. Design, paint, develop and animate.\n');
  for (const destination of ['media/hero.mp4', 'docs/media/hero.mp4', 'remotion/public/media/hero.mp4']) copy(film, destination);
  for (const id of ['design', 'pixel', 'motion', 'type']) {
    const source = path.join(output, `${id}-artwork.png`);
    run(binary, ['--convert', `examples/product-media/${id}.oma`, '--output', source], `export-${id}`);
    image(source, `site/public/media/showcase/${id}.webp`, ['-c:v', 'libwebp', '-lossless', '1']);
  }
  image(screenshots.photo, 'site/public/media/showcase/photo.webp', ['-c:v', 'libwebp', '-lossless', '1']);
  image(screenshots.photo, 'site/public/media/studio/raw-preview.webp', ['-c:v', 'libwebp', '-lossless', '1']);
  image(screenshots.templates, 'site/public/media/studio/templates.webp', ['-c:v', 'libwebp', '-lossless', '1']);
  for (const [id, scene] of clips) {
    const base = id === 'agent-panel' ? 'site/public/media/ai/agent-panel' : `site/public/media/recordings/${id}`;
    for (const ext of ['mp4', 'webm', 'webp', 'vtt']) receipts.push({ file: `${base}.${ext}`, sha256: hash(path.join(root, `${base}.${ext}`)), source: `native take ${scene}` });
  }
  for (const ext of ['mp4', 'webm', 'webp', 'vtt']) receipts.push({ file: `site/public/media/studio/film-current.${ext}`, sha256: hash(path.join(root, `site/public/media/studio/film-current.${ext}`)), source: 'fresh native replay edit' });
  const manifest = { film: { duration: 40, fps: 30, frames: 1200, width: 1600, height: 900, audio: false, shots: 20, edit: 'Context and detail cuts from actual native UI replays; canonical logo ending.' }, date: '2026-10-13', build: hash(binary), renderer: 'native WGPU on isolated Wayland display', sources: fs.readdirSync(path.join(root, 'examples/product-media')).filter(f => /\.(oma|svg|jpg)$/.test(f)).map(file => ({ file: `examples/product-media/${file}`, sha256: hash(path.join(root, 'examples/product-media', file)) })), clips: clips.map(([id, scene, duration, description]) => ({ id, scene, duration, description, sha256: hash(path.join(output, 'takes', id, `${scene}.mp4`)), fps: JSON.parse(fs.readFileSync(path.join(output, 'takes', id, `${scene}-capture.json`))).fps, unresolved_input_targets: JSON.parse(fs.readFileSync(path.join(output, 'takes', id, `${scene}-errors.json`))) })), outputs: [...new Map(receipts.map(receipt => [receipt.file, receipt])).values()] };
  fs.writeFileSync(path.join(root, 'site/public/media/manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  fs.writeFileSync(path.join(output, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  for (const destination of ['site/public/media/recordings/manifest.json', 'site/public/media/refresh/manifest.json', 'site/public/media/ai/agent-panel.json', 'site/public/media/studio/film-current.json']) fs.writeFileSync(path.join(root, destination), JSON.stringify(manifest, null, 2) + '\n');
}
