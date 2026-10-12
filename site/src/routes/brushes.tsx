import { createFileRoute } from '@tanstack/react-router';
import { sitePath } from '../site';
import { NIGHTLY_URL, NIGHTLY_VERSION } from '../release';
import './brushes.css';

export const Route = createFileRoute('/brushes')({
  head: () => ({ meta: [{ title: 'Brush packs — Omadesign' }, { name: 'description', content: '12 original painting tips. Download free Omadesign brush packs, import Photoshop brushes, and make brushes from your own artwork.' }] }),
  component: Brushes,
});

const packs = [
  { id: 'essentials', name: 'Essentials', subtitle: 'The marks you reach for.', brushes: ['Round', 'Soft', 'Flat', 'Calligraphy', 'Pencil', 'Dry ink'] },
  { id: 'texture', name: 'Texture', subtitle: 'Let the surface show.', brushes: ['Chalk', 'Charcoal', 'Grain', 'Stipple', 'Wash', 'Spatter'] },
];
function Brushes() {
  return <main id="main" className="shell brush-page">
    <p className="eyebrow">Made for Pixel</p>
    <h1>Make your mark.</h1>
    <p className="brush-intro">12 original brush tips. Yours to paint with, change, and share.</p>
    <p className="brush-proof">Original Omadesign artwork · CC0 · Free for personal and commercial work</p>
    <div className="brush-packs">{packs.map(pack => <article className="brush-pack" key={pack.id}>
      <h2>{pack.name}</h2><p>{pack.subtitle}</p>
      <img src={sitePath(`brushes/${pack.id}.svg`)} alt={`${pack.brushes.join(', ')} brush strokes`} width="640" height="360" />
      <ul>{pack.brushes.map(name => <li key={name}>{name}</li>)}</ul>
      <a className="brush-download" href={sitePath(`brushes/${pack.id}.omabrush`)} download>Download {pack.name} ↓</a>
    </article>)}</div>
    <section className="brush-guide"><div><h2>Bring your own.</h2>
      <p>In Pixel, choose Brush → Import. Pick a Photoshop ABR, Illustrator library, Omadesign pack, PNG, or SVG. Your source stays yours. Imported packs stay on your computer.</p>
      <p>Make a tip from a transparent image or dark artwork on white. Adjust its size, spacing, flow, and opacity. Save your settings or export a pack to share.</p>
    </div><div><h2>Start painting.</h2>
      <ol><li>Download a pack.</li><li>Open Pixel → Brush → Import.</li><li>Choose the pack, then select a brush.</li></ol>
      <p>Brush packs are included in <a href={NIGHTLY_URL}>Omadesign {NIGHTLY_VERSION}</a>. Download the Linux package for your computer, extract it, and open <code>omadesign</code>.</p>
    </div></section>
    <details className="brush-formats"><summary>Adobe import support</summary>
      <p>Photoshop ABR versions 1, 2, 6, 7, and 10: sampled 8/16-bit tips, raw or PackBits compression. Names and spacing come from descriptors when available. Photoshop dynamics, textures, tool presets, pressure, and scattering are not reproduced.</p>
      <p>Illustrator art and pattern libraries become sampled tips, including private libraries stored with zlib or Zstandard compression. Unsupported libraries can use expanded SVG artwork. Art, pattern, and bristle behavior is not preserved as editable vector strokes.</p>
      <p>Respect the license of any third-party brushes you import or share. <a href={sitePath('brushes/LICENSE.txt')}>Original pack license</a>.</p>
    </details>
  </main>;
}
