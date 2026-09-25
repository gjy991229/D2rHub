// Exercise the guide's real route/gallery handlers and check its packaged resources.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { JSDOM } = require('jsdom');

const root = path.resolve(__dirname, '..');
const guide = path.join(root, 'docs/user-guide.html');
const config = JSON.parse(fs.readFileSync(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
const resources = new Set(config.bundle.resources.map(file => path.resolve(root, 'src-tauri', file)));
const dom = new JSDOM(fs.readFileSync(guide, 'utf8'), {
  url: 'https://guide.local/user-guide.html', runScripts: 'outside-only', pretendToBeVisual: true,
});
const { window } = dom;
const runtimeErrors = [];
window.addEventListener('error', event => runtimeErrors.push(event.error || event.message));
window.HTMLElement.prototype.scrollIntoView = () => {};
const used = new Set([guide]);
function checkResource(relative) {
  assert(!/^(?:[a-z]+:|\/)/i.test(relative), `Unexpected guide resource: ${relative}`);
  const file = path.resolve(path.dirname(guide), relative);
  assert(fs.existsSync(file), `Missing guide resource: ${relative}`);
  assert(resources.has(file), `Guide resource not bundled: ${relative}`);
  used.add(file);
  return file;
}
try {
  checkResource('user-guide.html');
  for (const link of window.document.querySelectorAll('link[rel="stylesheet"]')) {
    checkResource(link.getAttribute('href'));
  }
  for (const script of window.document.querySelectorAll('script')) {
    window.eval(script.src
      ? fs.readFileSync(checkResource(script.getAttribute('src')), 'utf8')
      : script.textContent);
  }
  let steps = 0;
  function checkImages() {
    for (const image of window.document.querySelectorAll('#stepContent image')) {
      checkResource(image.getAttribute('href'));
    }
  }
  for (const route of ['multi', 'audio', 'room', 'all']) {
    window.location.hash = `guide-${route}`;
    window.dispatchEvent(new window.HashChangeEvent('hashchange'));
    const count = window.document.querySelectorAll('[data-step]').length;
    assert(count > 0, `Route did not render: ${route}`);
    for (let index = 0; index < count; index++) {
      window.document.querySelector(`[data-step="${index}"]`).click();
      assert(window.document.querySelector('#stepTitle'), `Missing step: ${route}/${index}`);
      checkImages();
      for (const button of window.document.querySelectorAll('[data-gallery-key]')) {
        button.click();
        checkImages();
      }
      steps++;
    }
  }
  const bundledFigures = [...resources].filter(file => file.includes(`${path.sep}guide-images${path.sep}`));
  for (const file of bundledFigures) assert(used.has(file), `Unused bundled guide resource: ${file}`);
  assert.equal(runtimeErrors.length, 0, `Guide runtime errors: ${runtimeErrors.join('; ')}`);
  console.log(`Guide OK: 4 routes, ${steps} steps, ${[...used].filter(file => file.endsWith('.png')).length} referenced screenshots; all resources bundled.`);
} finally {
  window.close();
}
