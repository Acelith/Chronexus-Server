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
const account = { accountId: 'self', displayName: 'Player', operator: false, gmEnabled: false };
const overview = { name: '<img src=x onerror=alert(1)>', role: 'owner', members: 1,
  myItems: [], myContentBytes: 0, myAttachments: 0, myAttachmentBytes: 0,
  sharedByOthers: 0, accountAttachmentBytes: 0, quotas: {} };

test('tables page keeps names inert and links to table workspace', async () => {
  const dom = fixture(async path => response(path.endsWith('/account') ? account :
    path.endsWith('/tables') ? [{ id: 'synthetic-id', name: '<img src=x onerror=alert(1)>', role: 'gm' }] : []));
  const { window: w } = dom;
  w.sessionStorage.setItem('chronexus-web-session', 'synthetic');
  w.location.hash = '#/tables';
  w.dispatchEvent(new w.HashChangeEvent('hashchange'));
  await settle();
  assert.equal(w.document.querySelector('#main img'), null);
  assert.equal(w.document.querySelector('#main a[href="#/table/synthetic-id"]').textContent, '<img src=x onerror=alert(1)>');
  assert.equal(w.document.querySelector('#nav a[aria-current="page"]').textContent, 'Tables');
  dom.window.close();
});

test('player table workspace has no empty GM controls or stray null text', async () => {
  const dom = fixture(async path => response(path.endsWith('/account') ? account :
    path.endsWith('/live-ticket') ? { ticket: 'synthetic' } :
    path.endsWith('/seats') ? [] :
    { id: 'synthetic-id', name: 'Synthetic table', role: 'player', revision: 1 }));
  const { window: w } = dom;
  w.WebSocket = class { close() {} };
  w.sessionStorage.setItem('chronexus-web-session', 'synthetic');
  w.location.hash = '#/table/synthetic-id';
  await settle();
  assert.equal(w.document.querySelector('main').textContent.includes('null'), false);
  assert.equal(w.document.querySelector('main').textContent.includes('Invite player'), false);
  assert.equal(w.document.querySelector('main h1').textContent, 'Synthetic table');
  dom.window.close();
});

test('Italian table workspace translates controls without changing the table name', async () => {
  const dom = fixture(async path => response(path.endsWith('/account') ? account :
    path.endsWith('/live-ticket') ? { ticket: 'synthetic' } :
    path.endsWith('/seats') ? [] :
    { id: 'synthetic-id', name: 'Original campaign text', role: 'player', revision: 1 }), { locale: 'it' });
  const { window: w } = dom;
  w.WebSocket = class { close() {} };
  w.sessionStorage.setItem('chronexus-web-session', 'synthetic');
  w.location.hash = '#/table/synthetic-id';
  await settle();
  assert.equal(w.document.querySelector('main h1').textContent, 'Original campaign text');
  assert.equal(w.document.querySelector('.live-heading h2').textContent, 'Canale del tavolo');
  assert.equal(w.document.querySelector('.table-roster h2').textContent, 'Al tavolo');
  dom.window.close();
});

for (const locale of ['en', 'it', 'ar']) {
  for (const outcome of ['uncommitted', 'committed', 'conflict']) {
    test(`${locale}: reconnect recovers a ${outcome} pending message`, async () => {
      const dom = fixture(async path => response(path.endsWith('/account') ? account :
        path.endsWith('/live-ticket') ? { ticket: 'synthetic' } :
        path.endsWith('/seats') ? [] :
        { id: 'synthetic-id', name: 'Table', role: 'player', revision: 1 }), { locale });
      try {
        const w = dom.window, sockets = [];
        class Socket {
          static OPEN = 1;
          readyState = 1;
          sent = [];
          constructor() { sockets.push(this); }
          send(value) { this.sent.push(JSON.parse(value)); }
          close() {}
        }
        w.WebSocket = Socket;
        const timeout = w.setTimeout.bind(w);
        w.setTimeout = (fn, ms) => timeout(fn, ms === 2000 ? 1 : ms);
        w.sessionStorage.setItem('chronexus-web-session', 'synthetic');
        w.location.hash = '#/table/synthetic-id';
        await settle();
        const first = sockets.at(-1);
        const state = (socket, revision, events = []) => socket.onmessage({ data: JSON.stringify({ type: 'state', revision, events, online: 1 }) });
        const input = w.document.querySelector('#live-message');
        const button = input.form.querySelector('button');
        first.onopen();
        assert.equal(button.disabled, true, 'wait for authoritative state');
        state(first, 1);
        input.value = 'Preserve this draft';
        input.form.dispatchEvent(new w.Event('submit', { cancelable: true }));
        const operation = first.sent[0];
        assert.ok(operation.operationId);
        // Enter cannot replace the pending operation while the button is disabled.
        input.form.dispatchEvent(new w.Event('submit', { cancelable: true }));
        assert.equal(first.sent.length, 1);
        first.onclose();
        await settle();
        const second = sockets.at(-1);
        assert.notEqual(second, first);
        second.onopen();
        const event = { ...operation, revision: 2, authorName: 'Player' };
        state(second, outcome === 'uncommitted' ? 1 : 2, outcome === 'committed' ? [event] : []);
        if (outcome === 'committed') {
          assert.equal(second.sent.length, 0, 'already committed messages are not resent');
          assert.equal(input.value, '');
        } else {
          assert.deepEqual(second.sent, [operation], 'retry exact ID, base revision and text');
          state(second, 2);
          assert.equal(second.sent.length, 1, 'retry once per connection');
          if (outcome === 'conflict') {
            second.onmessage({ data: JSON.stringify({ type: 'error', message: 'Revision conflict' }) });
            assert.equal(input.value, operation.text);
            assert.ok(w.document.querySelector('[role="alert"]'));
          } else {
            input.value = 'A newer draft typed while waiting';
            second.onmessage({ data: JSON.stringify({ type: 'ack', revision: 3 }) });
            assert.equal(input.value, 'A newer draft typed while waiting');
          }
        }
        assert.equal(button.disabled, false);
        first.onmessage({ data: JSON.stringify({ type: 'error' }) });
        assert.equal(button.disabled, false, 'stale socket callbacks are ignored');
        if (outcome === 'conflict') {
          input.form.dispatchEvent(new w.Event('submit', { cancelable: true }));
          assert.equal(second.sent.length, 2);
          assert.notEqual(second.sent[1].operationId, operation.operationId);
          assert.equal(second.sent[1].baseRevision, 2);
        }
        assert.equal(w.document.documentElement.dir, locale === 'ar' ? 'rtl' : 'ltr');
      } finally { dom.window.close(); }
    });
  }
}

test('GM workspace loads grants beyond the first 100 without showing untrusted markup', async () => {
  const requests = [];
  const grants = Array.from({ length: 101 }, (_, i) => ({
    campaignId: 'campaign', kind: 'page', itemId: String(i).padStart(3, '0'),
    authorName: 'Player', payload: { title: i === 100 ? '<img src=x onerror=alert(1)>' : `Item ${i}` },
  }));
  const dom = fixture(async path => {
    requests.push(path);
    return response(path.endsWith('/account') ? { ...account, gmEnabled: true } :
      path.includes('/journal-grants?after=') ? grants.slice(100) :
      path.endsWith('/journal-grants') ? grants.slice(0, 100) :
      path.endsWith('/seats') ? [] : path.endsWith('/live-ticket') ? { ticket: 'synthetic' } :
      { id: 'synthetic-id', name: 'Table', role: 'gm', revision: 1 });
  });
  try {
    const w = dom.window;
    w.WebSocket = class { close() {} };
    w.sessionStorage.setItem('chronexus-web-session', 'synthetic');
    w.location.hash = '#/table/synthetic-id';
    await settle();
    assert.equal(w.document.querySelectorAll('.gm-shared li').length, 101);
    assert.equal(w.document.querySelector('.gm-shared img'), null);
    assert.equal(w.document.querySelector('.gm-shared li:last-child strong').textContent, grants[100].payload.title);
    assert.equal(requests.filter(path => path.includes('/journal-grants')).length, 2);
    assert.ok(requests.some(path => path.endsWith('?after=campaign%2Fpage%2F099')));
  } finally { dom.window.close(); }
});

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
