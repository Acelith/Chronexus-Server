// Browser DOM regressions. Uses jsdom from the desktop development dependencies.
// Run with NODE_PATH pointing at Chronexus/node_modules, then node --test this file.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const { JSDOM } = require('jsdom');
const html = readFileSync(join(__dirname, '../web/index.html'), 'utf8');
const script = readFileSync(process.env.CHRONEXUS_WEB_SCRIPT || join(__dirname, '../web/app.js'), 'utf8');
const translations = readFileSync(join(__dirname, '../web/i18n.js'), 'utf8');
const settle = () => new Promise(resolve => setTimeout(resolve, 20));
function fixture(fetcher, options = {}) {
  const dom = new JSDOM(html, { url: 'http://localhost/', runScripts: 'outside-only' });
  dom.window.fetch = fetcher;
  dom.window.HTMLElement.prototype.scrollIntoView = () => {};
  if (options.locale) dom.window.localStorage.setItem('chronexus-web-locale', options.locale);
  dom.window.eval(translations);
  dom.window.eval(script);
  return dom;
}
const response = value => ({ status: 200, ok: true, text: async () => JSON.stringify(value) });
const account = { accountId: 'self', displayName: 'Player', operator: false };
const overview = { name: '<img src=x onerror=alert(1)>', role: 'owner', members: 1,
  myItems: [], myContentBytes: 0, myAttachments: 0, myAttachmentBytes: 0,
  sharedByOthers: 0, accountAttachmentBytes: 0, quotas: {} };

test('sign-in works in memory when tab storage is blocked', async () => {
  const dom = fixture(async (_, options) => response(options.method === 'POST' ? { token: 'synthetic' } : account));
  const { window: w } = dom;
  Object.defineProperty(w, 'sessionStorage', { get() { throw new Error('blocked'); } });
  w.fetch = async path => response(path.endsWith('/login') ? { token: 'synthetic' } : path.endsWith('/account') ? account : []);
  w.document.querySelector('#username').value = 'player';
  w.document.querySelector('#password').value = 'synthetic-password';
  w.document.querySelector('form').dispatchEvent(new w.Event('submit', { cancelable: true }));
  await settle();
  assert.match(w.document.querySelector('h1').textContent, /Your campaigns/);
  assert.equal(w.document.querySelector('#nav').hidden, false);
  dom.window.close();
});

test('campaign subpages keep navigation selected and names inert', async () => {
  const dom = fixture(async path => response(path.endsWith('/account') ? account :
    path.endsWith('/overview') ? overview : path.endsWith('/members') ? [] : []));
  const { window: w } = dom;
  w.sessionStorage.setItem('chronexus-web-session', 'synthetic');
  w.location.hash = '#/campaign/example/members';
  await settle();
  assert.equal(w.document.querySelector('h1').textContent, overview.name);
  assert.equal(w.document.querySelector('main img'), null);
  assert.equal(w.document.querySelector('#nav [aria-current]').textContent, 'Campaigns');
  assert.equal(w.document.querySelector('.panel-nav [aria-current]').textContent, 'Members');
  assert.equal(w.document.querySelector('.campaign-panel h2').textContent, 'Members');
  assert.equal(w.document.querySelector('.campaign-panel .grid'), null);
  dom.window.close();
});

test('a slow campaign response cannot replace the devices page', async () => {
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  const dom = fixture(async path => path.endsWith('/overview') ? pending :
    response(path.endsWith('/account') ? account : []));
  const { window: w } = dom;
  w.sessionStorage.setItem('chronexus-web-session', 'synthetic');
  w.location.hash = '#/campaign/example';
  await settle();
  w.location.hash = '#/devices';
  await settle();
  release(response(overview));
  await settle();
  assert.equal(w.document.querySelector('h1').textContent, 'Devices signed in');
  dom.window.close();
});

test('a non-JSON expired-session reply returns to sign-in', async () => {
  const dom = fixture(async () => ({ status: 401, ok: false, text: async () => 'Unauthorized' }));
  const { window: w } = dom;
  w.sessionStorage.setItem('chronexus-web-session', 'expired');
  w.location.hash = '#/devices';
  await settle();
  assert.equal(w.document.querySelector('h1').textContent, 'Sign in');
  assert.equal(w.sessionStorage.getItem('chronexus-web-session'), null);
  dom.window.close();
});

test('skip to content does not navigate away from the current subpage', async () => {
  const dom = fixture(async path => response(path.endsWith('/account') ? account : []));
  const { window: w } = dom;
  w.sessionStorage.setItem('chronexus-web-session', 'synthetic');
  w.location.hash = '#/devices';
  await settle();
  w.document.querySelector('.skip').click();
  await settle();
  assert.equal(w.location.hash, '#/devices');
  assert.equal(w.document.activeElement.id, 'main');
  dom.window.close();
});

test('every message has ten translations and placeholders survive', () => {
  const dom = fixture(async () => response(account));
  const i18n = dom.window.ChronexusI18n;
  assert.equal(i18n.missing().length, 0, i18n.missing().join(', '));
  const english = /\{(\w+)\}/g;
  for (const [code] of i18n.locales) {
    i18n.setLocale(code);
    assert.match(i18n.t('campaignMeta', { role: 'R', members: 3, date: 'D' }), /R.*3|3.*R/s);
    assert.doesNotMatch(i18n.t('limits', { items: 1, text: 2, images: 3, imageBytes: 4, account: 5, used: 6 }), english);
  }
  dom.window.close();
});

test('a saved language is used, Arabic is right-to-left and the menu switches back', async () => {
  const dom = fixture(async () => response(account), { locale: 'ar' });
  const { window: w } = dom;
  await settle();
  assert.equal(w.document.documentElement.dir, 'rtl');
  assert.equal(w.document.documentElement.lang, 'ar');
  assert.equal(w.document.querySelector('h1').textContent, 'تسجيل الدخول');
  const menu = w.document.querySelector('#language');
  assert.equal(menu.value, 'ar');
  menu.value = 'it';
  menu.dispatchEvent(new w.Event('change'));
  await settle();
  assert.equal(w.document.documentElement.dir, 'ltr');
  assert.equal(w.document.querySelector('h1').textContent, 'Accedi');
  assert.equal(w.document.querySelector('.skip').textContent, 'Vai al contenuto');
  assert.equal(w.localStorage.getItem('chronexus-web-locale'), 'it');
  dom.window.close();
});
