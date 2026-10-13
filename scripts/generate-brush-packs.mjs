import fs from 'node:fs';
import path from 'node:path';
const root = path.resolve(import.meta.dirname, '..');
const n = 96;
function tip(kind) {
  const data = Buffer.alloc(n * n);
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
    const a = (x + .5 - n / 2) / (n / 2), b = (y + .5 - n / 2) / (n / 2), r = Math.hypot(a, b);
    const noise = ((Math.imul(x + 31, 73856093) ^ Math.imul(y + 17, 19349663)) >>> 0) / 4294967295;
    let v = 0;
    switch (kind) {
      case 'Square': v = 1; break;
      case 'Round': v = Math.min(1, Math.max(0, (1 - r) * 24)); break;
      case 'Soft': v = Math.max(0, 1 - r) ** 2; break;
      case 'Flat': v = Math.max(0, Math.min(1, (1 - Math.abs(a)) * 20, (.22 - Math.abs(b)) * 30)); break;
      case 'Calligraphy': v = Math.max(0, Math.min(1, (1 - Math.abs((a+b)/1.4)) * 15, (.16 - Math.abs((a-b)/1.4)) * 30)); break;
      case 'Pencil': v = r < 1 ? Math.max(0, 1 - r) * (.2 + .8 * noise) : 0; break;
      case 'Chalk': v = r < 1 ? Math.min(1, (1-r)*12) * (noise > .36 ? .45 + .55*noise : 0) : 0; break;
      case 'Charcoal': v = r < 1 ? Math.max(0, 1-r) ** .4 * (noise > .25 ? noise : 0) : 0; break;
      case 'Dry ink': v = Math.abs(b) < .35 && Math.abs(a) < .96 && Math.sin(y * 7.2) > -.7 ? Math.max(0, 1 - Math.abs(a)) * (.4 + .6 * noise) : 0; break;
      case 'Grain': v = r < 1 && noise > .72 ? Math.max(0,1-r) : 0; break;
      case 'Stipple': v = r < 1 && noise > .94 ? 1 : 0; break;
      case 'Wash': v = r < 1 ? Math.max(0,1-r) ** .25 * (.22 + .2*noise) : 0; break;
      case 'Spatter': v = r < 1 && noise > .96 - .025*(1-r) ? .5 + .5*noise : 0; break;
    }
    data[y*n+x] = Math.round(v * 255);
  }
  return {width:n,height:n,alpha:data.toString('base64')};
}
const groups = [ ['essentials','Omadesign Essentials',['Round','Soft','Flat','Calligraphy','Pencil','Dry ink','Square']], ['texture','Omadesign Texture',['Chalk','Charcoal','Grain','Stipple','Wash','Spatter']] ];
for (const [id, name, kinds] of groups) {
  const pack = {format:'omadesign-brush-pack',version:1,name,author:'Omadesign',license:'CC0-1.0',notes:['Original procedural brush tips. Free to use, modify, and redistribute.'],brushes:kinds.map(name=>({name,size:name==='Pencil'?12:48,spacing:name==='Spatter'?.6:.12,hardness:1,flow:name==='Wash'?.45:1,opacity:1,tip:tip(name)}))};
  const file = `${id}.omabrush`;
  fs.mkdirSync(path.join(root,'assets/brushes'),{recursive:true});
  fs.writeFileSync(path.join(root,'assets/brushes',file),JSON.stringify(pack));
}
fs.mkdirSync(path.join(root, 'site/public/brushes'), { recursive: true });
for (const [id] of groups) fs.copyFileSync(path.join(root, `assets/brushes/${id}.omabrush`), path.join(root, `site/public/brushes/${id}.omabrush`));
