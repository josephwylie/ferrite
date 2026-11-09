// One fixture implementation for historical controller measurements and the
// packaged Ferrite acceptance runner. Oracle reads never mutate application data.
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';

const fixture = readFileSync(new URL('./fixture.html', import.meta.url));
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

export function createWorkOrderServer({records: initialRecords = []} = {}) {
  let records = structuredClone(initialRecords);
  const audit = [];
  const server = createServer(async (request, response) => {
    const url = new URL(request.url, 'http://127.0.0.1');
    response.setHeader('Content-Type', 'application/json');
    try {
      if (url.pathname === '/api/records' && request.method === 'POST') {
        let body = '';
        for await (const chunk of request) {
          body += chunk;
          if (body.length > 65536) throw Error('Record exceeds fixture limit');
        }
        const record = JSON.parse(body);
        await sleep(250);
        record.id = record.id || String(records.length + 1);
        records = records.filter(existing => existing.id !== record.id);
        records.push(record);
        audit.push({id: record.id, name: record.name, at: performance.now()});
        response.end(JSON.stringify(record));
      } else if (url.pathname === '/api/records' && request.method === 'GET') {
        await sleep(180);
        response.end(JSON.stringify(records));
      } else if (url.pathname === '/oracle' && request.method === 'GET') {
        response.end(JSON.stringify(records.find(record => record.name === url.searchParams.get('name')) || {}));
      } else if (url.pathname === '/audit' && request.method === 'GET') {
        response.end(JSON.stringify(audit));
      } else if (url.pathname === '/' && request.method === 'GET') {
        response.setHeader('Content-Type', 'text/html');
        response.end(fixture);
      } else {
        response.writeHead(404);
        response.end(JSON.stringify({error: 'Unknown fixture route'}));
      }
    } catch (error) {
      response.writeHead(400);
      response.end(JSON.stringify({error: String(error)}));
    }
  });
  return server;
}

// Standalone mode is deliberately only an HTTP fixture, never a browser driver.
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const server = createWorkOrderServer();
  server.listen(Number(process.env.FIXTURE_PORT || 0), '127.0.0.1', () => {
    console.log(JSON.stringify({url: `http://127.0.0.1:${server.address().port}`}));
  });
}
