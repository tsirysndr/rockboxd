import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { connectWith, rawImpl } from '../dagger-sdk/src/Dagger/Internal/Runtime.js';

async function fixture(t, timeoutMs) {
  const server = createServer(async (req, res) => {
    await delay(60);
    if (!res.destroyed) res.end(JSON.stringify({ data: { ok: true } }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  return connectWith({ port: server.address().port, token: 'test', timeoutMs })();
}
const execute = session => new Promise((resolve, reject) =>
  rawImpl(session)('query { ok }')({})(err => () => reject(err))(value => () => resolve(value))());

test('unlimited requests wait for delayed build responses', async t => {
  assert.deepEqual(await execute(await fixture(t, 0)), { ok: true });
});
test('explicit request deadlines still abort', async t => {
  await assert.rejects(execute(await fixture(t, 10)), /Dagger request timed out/);
});
test('cancellation suppresses completion callbacks', async t => {
  const session = await fixture(t, 0);
  let callbacks = 0;
  const cancel = rawImpl(session)('query { ok }')({})(() => () => callbacks++)(() => () => callbacks++)();
  cancel();
  await delay(90);
  assert.equal(callbacks, 0);
});
